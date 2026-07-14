//! Replay session: streams a recorded run's snapshots and events back to
//! clients at a configurable speed. The event log is the source of truth;
//! nothing is re-simulated, so what you replay is exactly what happened.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Result};
use tokio::sync::mpsc;

use dira_protocol::{Command, ServerMsg};

use crate::server::{AppState, ClientCommand};
use crate::store::RecordedRun;

const REPLAY_TICK_MS: u64 = 100;
const MIN_SPEED: f64 = 0.1;
const MAX_SPEED: f64 = 64.0;

pub struct ReplaySession {
    state: Arc<AppState>,
    run: RecordedRun,
    duration_ms: u64,
    position_ms: u64,
    speed: f64,
    playing: bool,
    next_event: usize,
    next_snapshot: usize,
    tick_count: u64,
}

impl ReplaySession {
    pub fn new(state: Arc<AppState>, run: RecordedRun, speed: f64) -> Result<Self> {
        if run.snapshots.is_empty() {
            bail!(
                "run {} has no recorded snapshots; nothing to replay",
                run.info.run_id
            );
        }
        let duration_ms = run
            .snapshots
            .last()
            .map(|(t, _)| *t)
            .max(run.events.last().map(|e| e.sim_time_ms))
            .unwrap_or(0);
        Ok(Self {
            state,
            run,
            duration_ms,
            position_ms: 0,
            speed: speed.clamp(MIN_SPEED, MAX_SPEED),
            playing: true,
            next_event: 0,
            next_snapshot: 0,
            tick_count: 0,
        })
    }

    pub async fn run(mut self, mut cmd_rx: mpsc::Receiver<ClientCommand>) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_millis(REPLAY_TICK_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        self.publish_status();

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick_count += 1;
                    if self.playing {
                        self.position_ms += (REPLAY_TICK_MS as f64 * self.speed) as u64;
                        self.emit_due();
                        if self.position_ms >= self.duration_ms {
                            self.position_ms = self.duration_ms;
                            self.playing = false;
                            self.publish_status();
                        }
                    }
                    if self.tick_count.is_multiple_of(10) {
                        self.publish_status();
                    }
                }
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => self.handle_command(cmd).await,
                    None => return Ok(()),
                }
            }
        }
    }

    fn emit_due(&mut self) {
        // Events first so the timeline explains the snapshot that follows.
        while self.next_event < self.run.events.len()
            && self.run.events[self.next_event].sim_time_ms <= self.position_ms
        {
            self.state.publish_event(&self.run.events[self.next_event]);
            self.next_event += 1;
        }
        let mut latest: Option<usize> = None;
        while self.next_snapshot < self.run.snapshots.len()
            && self.run.snapshots[self.next_snapshot].0 <= self.position_ms
        {
            latest = Some(self.next_snapshot);
            self.next_snapshot += 1;
        }
        if let Some(idx) = latest {
            self.state.publish_snapshot(&self.run.snapshots[idx].1);
        }
    }

    fn publish_status(&self) {
        self.state.publish_raw(ServerMsg::ReplayStatus {
            run_id: self.run.info.run_id,
            speed: self.speed,
            position_ms: self.position_ms,
            duration_ms: self.duration_ms,
            playing: self.playing,
        });
    }

    async fn handle_command(&mut self, cmd: ClientCommand) {
        let command_id = cmd.env.id;
        let result: Result<(), String> = match &cmd.env.command {
            Command::Pause => {
                self.playing = false;
                Ok(())
            }
            Command::Resume | Command::Start => {
                if self.position_ms >= self.duration_ms {
                    self.restart();
                }
                self.playing = true;
                Ok(())
            }
            Command::Reset => {
                self.restart();
                self.playing = true;
                Ok(())
            }
            Command::SetSpeed { multiplier } => {
                if multiplier.is_finite() && *multiplier > 0.0 {
                    self.speed = multiplier.clamp(MIN_SPEED, MAX_SPEED);
                    Ok(())
                } else {
                    Err("speed multiplier must be a positive number".into())
                }
            }
            _ => Err("this command is not available in replay mode".into()),
        };
        let reply = match result {
            Ok(()) => {
                self.publish_status();
                self.state.encode(ServerMsg::CommandAck { command_id })
            }
            Err(message) => self.state.encode(ServerMsg::Error {
                command_id: Some(command_id),
                code: "rejected".into(),
                message,
            }),
        };
        let _ = cmd.reply.send(reply).await;
    }

    fn restart(&mut self) {
        self.position_ms = 0;
        self.next_event = 0;
        self.next_snapshot = 0;
        self.state.hello.write().expect("hello lock").recent.clear();
        self.state.publish_snapshot(&self.run.snapshots[0].1);
    }
}
