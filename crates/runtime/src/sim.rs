//! The authoritative simulation session used by `simulate` and `edge` modes:
//! owns the engine and the event log, ticks on a fixed cadence, applies
//! operator commands, and publishes events/snapshots to clients. Keeps
//! running whether or not any browser is connected.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::sync::mpsc;

use dira_domain::{DomainEvent, Event, PolicyConfig, SimPhase};
use dira_protocol::{Command, ScenarioInfo, ServerMsg};
use dira_simulator::{engine::TICK_MS, load_scenario, OperatorAction, Scenario, SimEngine};

use crate::server::{AppState, ClientCommand};
use crate::store::Store;

/// Record a world snapshot to the event log at this sim-time granularity.
const SNAPSHOT_RECORD_MS: u64 = 200;

pub struct SimSession {
    state: Arc<AppState>,
    store: Store,
    engine: SimEngine,
    run_id: i64,
    run_finished: bool,
    scenarios: BTreeMap<String, PathBuf>,
    policy_override: Option<PolicyConfig>,
    tick_count: u64,
}

/// Scan the scenario file's directory for sibling scenarios so the operator
/// can switch between them. Files that do not parse as scenarios (for
/// example policy variant files) are skipped.
pub fn discover_scenarios(scenario_path: &Path) -> BTreeMap<String, PathBuf> {
    let mut found = BTreeMap::new();
    let dir = scenario_path.parent().unwrap_or(Path::new("."));
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                if let Ok(scenario) = load_scenario(&path) {
                    found.insert(scenario.id, path);
                }
            }
        }
    }
    found
}

pub fn apply_policy_override(scenario: &mut Scenario, policy: &Option<PolicyConfig>) {
    if let Some(p) = policy {
        scenario.policy = p.clone();
    }
}

impl SimSession {
    pub fn new(
        state: Arc<AppState>,
        store: Store,
        scenario: Scenario,
        scenario_path: &Path,
        policy_override: Option<PolicyConfig>,
    ) -> Result<Self> {
        let engine = SimEngine::new(scenario);
        let run_id = store.begin_run(engine.world())?;
        store.record_snapshot(run_id, engine.world())?;
        let scenarios = discover_scenarios(scenario_path);
        Ok(Self {
            state,
            store,
            engine,
            run_id,
            run_finished: false,
            scenarios,
            policy_override,
            tick_count: 0,
        })
    }

    pub fn scenario_infos(&self) -> Vec<ScenarioInfo> {
        self.scenarios
            .keys()
            .map(|id| {
                // Name lookup is best-effort; id doubles as a fallback name.
                let name = load_scenario(&self.scenarios[id])
                    .map(|s| s.name)
                    .unwrap_or_else(|_| id.clone());
                ScenarioInfo {
                    id: id.clone(),
                    name,
                }
            })
            .collect()
    }

    pub async fn run(mut self, mut cmd_rx: mpsc::Receiver<ClientCommand>) -> Result<()> {
        let initial = self.engine.initial_events();
        self.persist_and_publish(&initial)?;
        self.state.publish_snapshot(self.engine.world());

        let mut interval = tokio::time::interval(Duration::from_millis(TICK_MS));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    self.tick_count += 1;
                    let running = self.engine.world().phase == SimPhase::Running;
                    if running {
                        let events = self.engine.tick();
                        self.persist_and_publish(&events)?;
                        let world = self.engine.world();
                        if world.sim_time_ms.is_multiple_of(SNAPSHOT_RECORD_MS) {
                            self.store.record_snapshot(self.run_id, world)?;
                        }
                        self.state.publish_snapshot(world);
                        if events
                            .iter()
                            .any(|e| matches!(e.body, DomainEvent::ScenarioCompleted { .. }))
                        {
                            self.finish_run(true)?;
                        }
                    } else if self.tick_count.is_multiple_of(10) {
                        // 1 Hz heartbeat so clients can show liveness and
                        // staleness while idle, paused, or complete.
                        self.state.publish_snapshot(self.engine.world());
                    }
                }
                cmd = cmd_rx.recv() => match cmd {
                    Some(cmd) => self.handle_command(cmd).await?,
                    None => return Ok(()),
                }
            }
        }
    }

    fn persist_and_publish(&self, events: &[Event]) -> Result<()> {
        for event in events {
            self.store.record_event(self.run_id, event)?;
            self.state.publish_event(event);
        }
        Ok(())
    }

    fn finish_run(&mut self, completed: bool) -> Result<()> {
        if self.run_finished {
            return Ok(());
        }
        self.run_finished = true;
        let world = self.engine.world();
        self.store.record_snapshot(self.run_id, world)?;
        self.store.finish_run(
            self.run_id,
            completed,
            world.sim_time_ms,
            self.engine.final_metrics(),
        )?;
        self.refresh_runs()?;
        Ok(())
    }

    fn refresh_runs(&self) -> Result<()> {
        let runs = self.store.list_runs()?;
        self.state.hello.write().expect("hello lock").runs = runs;
        Ok(())
    }

    /// Swap in a fresh engine (reset or scenario switch) and open a new run.
    fn replace_engine(&mut self, scenario: Scenario) -> Result<()> {
        self.finish_run(self.engine.world().phase == SimPhase::Complete)?;
        self.engine = SimEngine::new(scenario);
        self.run_id = self.store.begin_run(self.engine.world())?;
        self.store
            .record_snapshot(self.run_id, self.engine.world())?;
        self.run_finished = false;
        self.state.hello.write().expect("hello lock").recent.clear();
        self.refresh_runs()?;
        let initial = self.engine.initial_events();
        self.persist_and_publish(&initial)?;
        self.state.publish_snapshot(self.engine.world());
        Ok(())
    }

    async fn handle_command(&mut self, cmd: ClientCommand) -> Result<()> {
        let command_id = cmd.env.id;
        let result: Result<(), String> = match &cmd.env.command {
            Command::Start => self.apply(OperatorAction::Start),
            Command::Pause => self.apply(OperatorAction::Pause),
            Command::Resume => self.apply(OperatorAction::Resume),
            Command::AckRecommendation { id } => self.apply(OperatorAction::Ack(id.clone())),
            Command::ApproveRecommendation { id } => {
                self.apply(OperatorAction::Approve(id.clone()))
            }
            Command::DeclineRecommendation { id } => {
                self.apply(OperatorAction::Decline(id.clone()))
            }
            Command::InjectFault { fault } => {
                self.apply(OperatorAction::InjectFault(fault.clone()))
            }
            Command::Reset => {
                let scenario = self.engine.scenario().clone();
                self.replace_engine(scenario).map_err(|e| e.to_string())
            }
            Command::SelectScenario { scenario_id } => match self.scenarios.get(scenario_id) {
                Some(path) => match load_scenario(path) {
                    Ok(mut scenario) => {
                        apply_policy_override(&mut scenario, &self.policy_override);
                        self.replace_engine(scenario).map_err(|e| e.to_string())
                    }
                    Err(e) => Err(format!("scenario failed to load: {e}")),
                },
                None => Err(format!("unknown scenario '{scenario_id}'")),
            },
            Command::SetSpeed { .. } => Err("set_speed is only available in replay mode".into()),
        };

        let reply = match result {
            Ok(()) => {
                self.state.publish_snapshot(self.engine.world());
                self.state.encode(ServerMsg::CommandAck { command_id })
            }
            Err(message) => {
                tracing::warn!("command {command_id} rejected: {message}");
                self.state.encode(ServerMsg::Error {
                    command_id: Some(command_id),
                    code: "rejected".into(),
                    message,
                })
            }
        };
        let _ = cmd.reply.send(reply).await;
        Ok(())
    }

    fn apply(&mut self, action: OperatorAction) -> Result<(), String> {
        let events = self.engine.apply(action, "operator")?;
        self.persist_and_publish(&events).map_err(|e| e.to_string())
    }
}

pub fn load_policy_file(path: &Path) -> Result<PolicyConfig> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading policy file {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parsing policy file {}", path.display()))
}
