use std::collections::{BTreeMap, VecDeque};

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use dira_domain::{
    evaluate_policies, AssetStatus, Assignment, AssignmentStatus, DomainEvent, Event, FaultSpec,
    LinkState, MetricsAccumulator, Objective, PolicyMemory, RecKind, RecStatus, RunMetrics,
    SimPhase, Track, TrackClass, TrackStatus, Vec2, WorldState,
};

use crate::scenario::{Motion, Scenario};

pub const TICK_MS: u64 = 100;

const UNCERTAINTY_BASE_M: f64 = 20.0;
const UNCERTAINTY_GROWTH_M_PER_S: f64 = 15.0;
const UNCERTAINTY_CAP_M: f64 = 500.0;
const STALE_AFTER_MS: u64 = 3_000;
const LOST_AFTER_MS: u64 = 20_000;
const DROP_AFTER_MS: u64 = 45_000;
/// An investigating asset within this range of the track is "on station".
const INVESTIGATE_RANGE_M: f64 = 100.0;
/// Cumulative on-station time to complete an investigation.
const INVESTIGATE_DWELL_MS: u64 = 10_000;

/// Ground-truth motion state for one planned track. Never leaves the engine;
/// clients only ever see link-mediated observations of it.
#[derive(Debug, Clone)]
struct TruthTrack {
    plan_idx: usize,
    spawned: bool,
    done: bool,
    pos: Vec2,
    vel: Vec2,
    wp: usize,
    orbit_ang: Option<f64>,
    heading: f64,
    next_turn_ms: u64,
}

#[derive(Debug, Clone)]
struct Obs {
    track: String,
    class: TrackClass,
    pos: Vec2,
    vel: Vec2,
    via_link: String,
    measured_ms: u64,
}

/// Per-asset navigation state (patrol progress).
#[derive(Debug, Clone)]
struct AssetNav {
    patrol: Vec<Vec2>,
    wp: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OperatorAction {
    Start,
    Pause,
    Resume,
    Ack(String),
    Approve(String),
    Decline(String),
    InjectFault(FaultSpec),
}

pub struct SimEngine {
    scenario: Scenario,
    rng: ChaCha8Rng,
    world: WorldState,
    truth: Vec<TruthTrack>,
    asset_nav: Vec<AssetNav>,
    pending_obs: VecDeque<(u64, Obs)>,
    investigate_dwell: BTreeMap<String, u64>,
    next_fault: usize,
    event_seq: u64,
    rec_seq: u64,
    asg_seq: u64,
    emitted_loaded: bool,
    policy_mem: PolicyMemory,
    metrics: MetricsAccumulator,
    final_metrics: Option<RunMetrics>,
}

impl SimEngine {
    pub fn new(scenario: Scenario) -> Self {
        let mut world = WorldState {
            scenario_id: scenario.id.clone(),
            scenario_name: scenario.name.clone(),
            seed: scenario.seed,
            phase: SimPhase::Idle,
            sim_time_ms: 0,
            duration_ms: scenario.duration_ms,
            bounds: scenario.bounds,
            zones: scenario.zones.clone(),
            assets: scenario.assets.iter().map(|a| a.asset.clone()).collect(),
            tracks: Vec::new(),
            links: scenario.links.clone(),
            nodes: scenario.nodes.clone(),
            recommendations: Vec::new(),
            assignments: Vec::new(),
            basemap: scenario.basemap.clone(),
        };
        // Initial coverage flags so the very first snapshot is accurate.
        for zone in &mut world.zones {
            zone.covered = scenario
                .assets
                .iter()
                .any(|a| zone.center.dist(a.asset.pos) <= zone.radius_m);
        }
        let truth = scenario
            .tracks
            .iter()
            .enumerate()
            .map(|(i, plan)| TruthTrack {
                plan_idx: i,
                spawned: false,
                done: false,
                pos: plan.start,
                vel: Vec2::default(),
                wp: 0,
                orbit_ang: None,
                heading: 0.0,
                next_turn_ms: 0,
            })
            .collect();
        let asset_nav = scenario
            .assets
            .iter()
            .map(|a| AssetNav {
                patrol: a.patrol.clone(),
                wp: 0,
            })
            .collect();
        Self {
            rng: ChaCha8Rng::seed_from_u64(scenario.seed),
            world,
            truth,
            asset_nav,
            pending_obs: VecDeque::new(),
            investigate_dwell: BTreeMap::new(),
            next_fault: 0,
            event_seq: 0,
            rec_seq: 0,
            asg_seq: 0,
            emitted_loaded: false,
            policy_mem: PolicyMemory::default(),
            metrics: MetricsAccumulator::default(),
            final_metrics: None,
            scenario,
        }
    }

    pub fn world(&self) -> &WorldState {
        &self.world
    }

    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    pub fn final_metrics(&self) -> Option<&RunMetrics> {
        self.final_metrics.as_ref()
    }

    /// The ScenarioLoaded event; call once after construction.
    pub fn initial_events(&mut self) -> Vec<Event> {
        if self.emitted_loaded {
            return Vec::new();
        }
        self.emitted_loaded = true;
        let mut out = Vec::new();
        self.emit(
            &mut out,
            DomainEvent::ScenarioLoaded {
                scenario_id: self.scenario.id.clone(),
                name: self.scenario.name.clone(),
                seed: self.scenario.seed,
            },
        );
        out
    }

    fn emit(&mut self, out: &mut Vec<Event>, body: DomainEvent) {
        self.event_seq += 1;
        let event = Event {
            seq: self.event_seq,
            sim_time_ms: self.world.sim_time_ms,
            body,
        };
        self.metrics.on_event(&event);
        out.push(event);
    }

    pub fn apply(&mut self, action: OperatorAction, by: &str) -> Result<Vec<Event>, String> {
        let mut out = Vec::new();
        match action {
            OperatorAction::Start => {
                if self.world.phase != SimPhase::Idle {
                    return Err(format!("cannot start from {:?}", self.world.phase));
                }
                self.world.phase = SimPhase::Running;
                self.emit(
                    &mut out,
                    DomainEvent::PhaseChanged {
                        phase: SimPhase::Running,
                    },
                );
            }
            OperatorAction::Pause => {
                if self.world.phase != SimPhase::Running {
                    return Err(format!("cannot pause from {:?}", self.world.phase));
                }
                self.world.phase = SimPhase::Paused;
                self.emit(
                    &mut out,
                    DomainEvent::PhaseChanged {
                        phase: SimPhase::Paused,
                    },
                );
            }
            OperatorAction::Resume => {
                if self.world.phase != SimPhase::Paused {
                    return Err(format!("cannot resume from {:?}", self.world.phase));
                }
                self.world.phase = SimPhase::Running;
                self.emit(
                    &mut out,
                    DomainEvent::PhaseChanged {
                        phase: SimPhase::Running,
                    },
                );
            }
            OperatorAction::Ack(id) => {
                let now = self.world.sim_time_ms;
                let rec = self
                    .world
                    .recommendation_mut(&id)
                    .ok_or_else(|| format!("unknown recommendation {id}"))?;
                if rec.status != RecStatus::Pending {
                    return Err(format!("recommendation {id} is not pending"));
                }
                rec.status = RecStatus::Acknowledged;
                rec.status_changed_ms = now;
                self.emit(
                    &mut out,
                    DomainEvent::RecommendationStatusChanged {
                        id,
                        status: RecStatus::Acknowledged,
                        by: by.to_string(),
                    },
                );
            }
            OperatorAction::Approve(id) => {
                return self.approve(&id, by);
            }
            OperatorAction::Decline(id) => {
                let now = self.world.sim_time_ms;
                let rec = self
                    .world
                    .recommendation_mut(&id)
                    .ok_or_else(|| format!("unknown recommendation {id}"))?;
                if !rec.status.is_open() {
                    return Err(format!("recommendation {id} is not open"));
                }
                rec.status = RecStatus::Declined;
                rec.status_changed_ms = now;
                let key = rec.kind.dedupe_key();
                self.policy_mem.mark_resolved(key);
                self.emit(
                    &mut out,
                    DomainEvent::RecommendationStatusChanged {
                        id,
                        status: RecStatus::Declined,
                        by: by.to_string(),
                    },
                );
            }
            OperatorAction::InjectFault(fault) => {
                self.validate_fault(&fault)?;
                self.emit(
                    &mut out,
                    DomainEvent::FaultInjected {
                        by: by.to_string(),
                        fault: fault.clone(),
                    },
                );
                self.apply_fault(&fault, &mut out);
            }
        }
        Ok(out)
    }

    fn validate_fault(&self, fault: &FaultSpec) -> Result<(), String> {
        match fault {
            FaultSpec::Link { link, .. } => {
                if !self.world.links.iter().any(|l| &l.id == link) {
                    return Err(format!("unknown link {link}"));
                }
            }
            FaultSpec::Node { node, .. } => {
                if !self.world.nodes.iter().any(|n| &n.id == node) {
                    return Err(format!("unknown node {node}"));
                }
            }
        }
        Ok(())
    }

    fn approve(&mut self, id: &str, by: &str) -> Result<Vec<Event>, String> {
        let now = self.world.sim_time_ms;
        let rec = self
            .world
            .recommendation(id)
            .ok_or_else(|| format!("unknown recommendation {id}"))?
            .clone();
        if !rec.status.is_open() {
            return Err(format!("recommendation {id} is not open"));
        }

        let mut out = Vec::new();
        // Validate and perform the side effect first; approval fails closed if
        // the world has moved on since the recommendation was issued.
        match &rec.kind {
            RecKind::AssignObservation { asset, zone } => {
                let objective = Objective::ObserveZone { zone: zone.clone() };
                self.create_assignment(asset, objective, id, &mut out)?;
            }
            RecKind::InvestigateTrack { asset, track } => {
                match self.world.track(track) {
                    Some(t) if t.status != TrackStatus::Lost => {}
                    _ => return Err(format!("track {track} is no longer trackable")),
                }
                let objective = Objective::InvestigateTrack {
                    track: track.clone(),
                };
                self.create_assignment(asset, objective, id, &mut out)?;
            }
            RecKind::FlagTrack { track, .. } => {
                let t = self
                    .world
                    .tracks
                    .iter_mut()
                    .find(|t| &t.id == track)
                    .ok_or_else(|| format!("track {track} no longer exists"))?;
                t.flagged = true;
            }
            RecKind::CommsCheck { .. } => {}
        }

        let rec_mut = self.world.recommendation_mut(id).expect("checked above");
        rec_mut.status = RecStatus::Approved;
        rec_mut.status_changed_ms = now;
        self.policy_mem.mark_resolved(rec.kind.dedupe_key());
        self.emit(
            &mut out,
            DomainEvent::RecommendationStatusChanged {
                id: id.to_string(),
                status: RecStatus::Approved,
                by: by.to_string(),
            },
        );
        Ok(out)
    }

    fn create_assignment(
        &mut self,
        asset_id: &str,
        objective: Objective,
        from_rec: &str,
        out: &mut Vec<Event>,
    ) -> Result<(), String> {
        let now = self.world.sim_time_ms;
        let asset = self
            .world
            .asset(asset_id)
            .ok_or_else(|| format!("unknown asset {asset_id}"))?;
        if asset.status != AssetStatus::Available {
            return Err(format!("asset {asset_id} is no longer available"));
        }
        self.asg_seq += 1;
        let assignment = Assignment {
            id: format!("A-{:04}", self.asg_seq),
            asset: asset_id.to_string(),
            objective,
            created_ms: now,
            status: AssignmentStatus::Active,
            from_recommendation: from_rec.to_string(),
        };
        let asset = self.world.asset_mut(asset_id).expect("checked above");
        asset.status = AssetStatus::Enroute;
        asset.assignment = Some(assignment.id.clone());
        self.world.assignments.push(assignment.clone());
        self.emit(out, DomainEvent::AssignmentCreated { assignment });
        Ok(())
    }

    fn apply_fault(&mut self, fault: &FaultSpec, out: &mut Vec<Event>) {
        let now = self.world.sim_time_ms;
        match fault {
            FaultSpec::Link { link, state } => {
                if let Some(l) = self.world.link_mut(link) {
                    if l.state != *state {
                        l.state = *state;
                        l.since_ms = now;
                        let (link, state) = (link.clone(), *state);
                        self.emit(out, DomainEvent::LinkStateChanged { link, state });
                    }
                }
            }
            FaultSpec::Node { node, health } => {
                if let Some(n) = self.world.nodes.iter_mut().find(|n| &n.id == node) {
                    if n.health != *health {
                        n.health = *health;
                        let (node, health) = (node.clone(), *health);
                        self.emit(out, DomainEvent::NodeHealthChanged { node, health });
                    }
                }
            }
        }
    }

    /// Advance one fixed timestep. No-op unless running.
    pub fn tick(&mut self) -> Vec<Event> {
        if self.world.phase != SimPhase::Running {
            return Vec::new();
        }
        let mut out = Vec::new();
        self.world.sim_time_ms += TICK_MS;
        let now = self.world.sim_time_ms;
        let dt_s = TICK_MS as f64 / 1000.0;

        // 1. Scheduled scenario faults.
        while self.next_fault < self.scenario.faults.len()
            && self.scenario.faults[self.next_fault].at_ms <= now
        {
            let timed = self.scenario.faults[self.next_fault].clone();
            self.next_fault += 1;
            self.emit(
                &mut out,
                DomainEvent::FaultInjected {
                    by: "scenario".to_string(),
                    fault: timed.fault.clone(),
                },
            );
            self.apply_fault(&timed.fault, &mut out);
        }

        // 2. Ground truth motion.
        self.advance_truth(now, dt_s);

        // 3. Observations through links (this is where degraded links bite).
        self.generate_observations(now);
        self.deliver_observations(now, &mut out);

        // 4. Staleness, uncertainty, and track lifecycle.
        self.age_tracks(now, &mut out);

        // 5. Assets execute assignments or patrol.
        self.advance_assets(now, dt_s, &mut out);

        // 6. Zone coverage.
        self.update_coverage(&mut out);

        // 7. Expire stale recommendations.
        self.expire_recommendations(now, &mut out);

        // 8. Policy evaluation.
        let new_recs = evaluate_policies(
            &self.world,
            &self.scenario.policy,
            &mut self.policy_mem,
            &mut self.rec_seq,
        );
        for rec in new_recs {
            self.world.recommendations.push(rec.clone());
            self.emit(
                &mut out,
                DomainEvent::RecommendationIssued {
                    recommendation: rec,
                },
            );
        }

        // 9. Metrics.
        self.metrics.on_tick(&self.world, TICK_MS);

        // 10. Completion.
        if now >= self.world.duration_ms {
            self.world.phase = SimPhase::Complete;
            self.emit(
                &mut out,
                DomainEvent::PhaseChanged {
                    phase: SimPhase::Complete,
                },
            );
            let metrics = self.metrics.finish(&self.world);
            self.final_metrics = Some(metrics.clone());
            self.emit(&mut out, DomainEvent::ScenarioCompleted { metrics });
        }
        out
    }

    fn advance_truth(&mut self, now: u64, dt_s: f64) {
        let bounds = self.world.bounds;
        for t in &mut self.truth {
            let plan = &self.scenario.tracks[t.plan_idx];
            if t.done {
                continue;
            }
            if !t.spawned {
                if now >= plan.enter_at_ms {
                    t.spawned = true;
                    t.pos = plan.start;
                    if matches!(plan.motion, Motion::Wander) {
                        t.heading = self.rng.gen_range(0.0..std::f64::consts::TAU);
                        t.next_turn_ms = now + self.rng.gen_range(2000..6000);
                    }
                } else {
                    continue;
                }
            }
            match &plan.motion {
                Motion::Patrol { waypoints } => {
                    let target = waypoints[t.wp % waypoints.len()];
                    let (pos, vel, arrived) = step_toward(t.pos, target, plan.speed, dt_s);
                    t.pos = pos;
                    t.vel = vel;
                    if arrived {
                        t.wp = (t.wp + 1) % waypoints.len();
                    }
                }
                Motion::Transit { exit } => {
                    let (pos, vel, arrived) = step_toward(t.pos, *exit, plan.speed, dt_s);
                    t.pos = pos;
                    t.vel = vel;
                    if arrived {
                        t.done = true;
                    }
                }
                Motion::Route { waypoints, loiter } => {
                    let last = waypoints.len() - 1;
                    let target = waypoints[t.wp.min(last)];
                    let (pos, vel, arrived) = step_toward(t.pos, target, plan.speed, dt_s);
                    t.pos = pos;
                    t.vel = vel;
                    if arrived {
                        if t.wp >= last {
                            if *loiter {
                                t.vel = Vec2::default();
                            } else {
                                t.done = true;
                            }
                        } else {
                            t.wp += 1;
                        }
                    }
                }
                Motion::Incursion { zone } => {
                    let (center, radius) = self
                        .world
                        .zones
                        .iter()
                        .find(|z| &z.id == zone)
                        .map(|z| (z.center, z.radius_m))
                        .expect("validated at load");
                    let orbit_r = radius * 0.5;
                    if t.orbit_ang.is_none() && t.pos.dist(center) > orbit_r {
                        let (pos, vel, _) = step_toward(t.pos, center, plan.speed, dt_s);
                        t.pos = pos;
                        t.vel = vel;
                    } else {
                        let ang = t.orbit_ang.unwrap_or_else(|| {
                            let rel = t.pos.sub(center);
                            rel.y.atan2(rel.x)
                        });
                        let w = plan.speed / orbit_r;
                        let ang = ang + w * dt_s;
                        t.orbit_ang = Some(ang);
                        t.pos = center.add(Vec2::new(ang.cos() * orbit_r, ang.sin() * orbit_r));
                        t.vel = Vec2::new(-ang.sin(), ang.cos()).scale(plan.speed);
                    }
                }
                Motion::Wander => {
                    if now >= t.next_turn_ms {
                        t.heading += self.rng.gen_range(-1.0..1.0) * std::f64::consts::FRAC_PI_3;
                        t.next_turn_ms = now + self.rng.gen_range(2000..6000);
                    }
                    t.vel = Vec2::new(t.heading.cos(), t.heading.sin()).scale(plan.speed);
                    let mut pos = t.pos.add(t.vel.scale(dt_s));
                    let (hw, hh) = (bounds.width / 2.0, bounds.height / 2.0);
                    if pos.x.abs() > hw {
                        t.heading = std::f64::consts::PI - t.heading;
                        pos.x = pos.x.clamp(-hw, hw);
                    }
                    if pos.y.abs() > hh {
                        t.heading = -t.heading;
                        pos.y = pos.y.clamp(-hh, hh);
                    }
                    t.pos = pos;
                }
            }
            if !bounds.contains(t.pos)
                && matches!(
                    plan.motion,
                    Motion::Transit { .. } | Motion::Route { loiter: false, .. }
                )
            {
                t.done = true;
            }
        }
    }

    fn generate_observations(&mut self, now: u64) {
        for t in &self.truth {
            if !t.spawned || t.done {
                continue;
            }
            let plan = &self.scenario.tracks[t.plan_idx];
            let link_state = self
                .world
                .links
                .iter()
                .find(|l| l.id == plan.via_link)
                .map(|l| l.state)
                .unwrap_or(LinkState::Nominal);
            let obs = Obs {
                track: plan.id.clone(),
                class: plan.class,
                pos: t.pos,
                vel: t.vel,
                via_link: plan.via_link.clone(),
                measured_ms: now,
            };
            match link_state {
                LinkState::Nominal => self.pending_obs.push_back((now, obs)),
                LinkState::Delayed { delay_ms } => {
                    self.pending_obs.push_back((now + delay_ms, obs))
                }
                LinkState::Intermittent { loss } => {
                    // One deterministic RNG draw per spawned track per tick.
                    if self.rng.gen::<f64>() >= loss {
                        self.pending_obs.push_back((now, obs));
                    }
                }
                LinkState::Unavailable => {
                    // Still consume a draw so link recovery does not shift the
                    // stream consumed by other tracks' intermittent rolls.
                    let _ = self.rng.gen::<f64>();
                }
            }
        }
    }

    fn deliver_observations(&mut self, now: u64, out: &mut Vec<Event>) {
        let mut due = Vec::new();
        self.pending_obs.retain(|(at, obs)| {
            if *at <= now {
                due.push(obs.clone());
                false
            } else {
                true
            }
        });
        // Index once per tick; a linear find() per observation is O(n^2)
        // per tick and measurably blows the tick budget at 10k tracks.
        let mut index: std::collections::HashMap<String, usize> = self
            .world
            .tracks
            .iter()
            .enumerate()
            .map(|(i, t)| (t.id.clone(), i))
            .collect();
        for obs in due {
            if let Some(track) = index
                .get(obs.track.as_str())
                .map(|&i| &mut self.world.tracks[i])
            {
                // A delayed observation can arrive after a fresher one; never
                // regress the picture.
                if obs.measured_ms >= track.last_seen_ms {
                    track.pos = obs.pos;
                    track.vel = obs.vel;
                    track.last_seen_ms = obs.measured_ms;
                }
            } else {
                let track = Track {
                    id: obs.track.clone(),
                    class: obs.class,
                    pos: obs.pos,
                    vel: obs.vel,
                    status: TrackStatus::Active,
                    uncertainty_m: UNCERTAINTY_BASE_M,
                    last_seen_ms: obs.measured_ms,
                    via_link: obs.via_link.clone(),
                    flagged: false,
                };
                index.insert(obs.track.clone(), self.world.tracks.len());
                self.world.tracks.push(track);
                self.emit(
                    out,
                    DomainEvent::TrackAppeared {
                        track: obs.track,
                        class: obs.class,
                    },
                );
            }
        }
    }

    fn age_tracks(&mut self, now: u64, out: &mut Vec<Event>) {
        let mut status_changes = Vec::new();
        let mut dropped = Vec::new();
        self.world.tracks.retain_mut(|track| {
            let age = now.saturating_sub(track.last_seen_ms);
            track.uncertainty_m = (UNCERTAINTY_BASE_M
                + UNCERTAINTY_GROWTH_M_PER_S * age as f64 / 1000.0)
                .min(UNCERTAINTY_CAP_M);
            let status = if age <= STALE_AFTER_MS {
                TrackStatus::Active
            } else if age <= LOST_AFTER_MS {
                TrackStatus::Stale
            } else {
                TrackStatus::Lost
            };
            if status != track.status {
                track.status = status;
                status_changes.push((track.id.clone(), status));
            }
            if age > DROP_AFTER_MS {
                dropped.push(track.id.clone());
                false
            } else {
                true
            }
        });
        for (track, status) in status_changes {
            self.emit(out, DomainEvent::TrackStatusChanged { track, status });
        }
        for track in dropped {
            self.emit(out, DomainEvent::TrackDropped { track });
        }
    }

    fn advance_assets(&mut self, _now: u64, dt_s: f64, out: &mut Vec<Event>) {
        let zone_pos: BTreeMap<String, (Vec2, f64)> = self
            .world
            .zones
            .iter()
            .map(|z| (z.id.clone(), (z.center, z.radius_m)))
            .collect();
        let track_pos: BTreeMap<String, (Vec2, TrackStatus)> = self
            .world
            .tracks
            .iter()
            .map(|t| (t.id.clone(), (t.pos, t.status)))
            .collect();

        let mut completions: Vec<(String, String)> = Vec::new(); // (assignment id, outcome)

        for (idx, asset) in self.world.assets.iter_mut().enumerate() {
            let nav = &mut self.asset_nav[idx];
            let assignment = asset.assignment.as_ref().and_then(|id| {
                self.world
                    .assignments
                    .iter()
                    .find(|a| &a.id == id && a.status == AssignmentStatus::Active)
                    .cloned()
            });
            match assignment {
                Some(assignment) => match &assignment.objective {
                    Objective::ObserveZone { zone } => {
                        let (center, radius) = zone_pos[zone];
                        let (pos, vel, _) = step_toward(asset.pos, center, asset.speed_mps, dt_s);
                        asset.pos = pos;
                        asset.vel = vel;
                        asset.status = if asset.pos.dist(center) <= radius * 0.5 {
                            AssetStatus::Observing
                        } else {
                            AssetStatus::Enroute
                        };
                    }
                    Objective::InvestigateTrack { track } => match track_pos.get(track) {
                        Some((tpos, tstatus)) if *tstatus != TrackStatus::Lost => {
                            let (pos, vel, _) =
                                step_toward(asset.pos, *tpos, asset.speed_mps, dt_s);
                            asset.pos = pos;
                            asset.vel = vel;
                            if asset.pos.dist(*tpos) <= INVESTIGATE_RANGE_M {
                                asset.status = AssetStatus::Investigating;
                                let dwell = self
                                    .investigate_dwell
                                    .entry(assignment.id.clone())
                                    .or_insert(0);
                                *dwell += TICK_MS;
                                if *dwell >= INVESTIGATE_DWELL_MS {
                                    completions
                                        .push((assignment.id.clone(), "investigated".into()));
                                    asset.status = AssetStatus::Available;
                                    asset.assignment = None;
                                }
                            } else {
                                asset.status = AssetStatus::Enroute;
                            }
                        }
                        _ => {
                            completions.push((assignment.id.clone(), "track lost".into()));
                            asset.status = AssetStatus::Available;
                            asset.assignment = None;
                            asset.vel = Vec2::default();
                        }
                    },
                },
                None => {
                    // Patrol loop, or hold position.
                    if !nav.patrol.is_empty() {
                        let target = nav.patrol[nav.wp % nav.patrol.len()];
                        let (pos, vel, arrived) =
                            step_toward(asset.pos, target, asset.speed_mps, dt_s);
                        asset.pos = pos;
                        asset.vel = vel;
                        if arrived {
                            nav.wp = (nav.wp + 1) % nav.patrol.len();
                        }
                    } else {
                        asset.vel = Vec2::default();
                    }
                }
            }
        }

        for (id, outcome) in completions {
            if let Some(a) = self.world.assignments.iter_mut().find(|a| a.id == id) {
                a.status = AssignmentStatus::Completed;
            }
            self.investigate_dwell.remove(&id);
            self.emit(out, DomainEvent::AssignmentCompleted { id, outcome });
        }
    }

    fn update_coverage(&mut self, out: &mut Vec<Event>) {
        let mut changes = Vec::new();
        for zone in &mut self.world.zones {
            let covered = self.world.assets.iter().any(|a| {
                a.status != AssetStatus::Unavailable && zone.center.dist(a.pos) <= zone.radius_m
            });
            if covered != zone.covered {
                zone.covered = covered;
                changes.push((zone.id.clone(), covered));
            }
        }
        for (zone, covered) in changes {
            self.emit(out, DomainEvent::ZoneCoverageChanged { zone, covered });
        }
    }

    fn expire_recommendations(&mut self, now: u64, out: &mut Vec<Event>) {
        let mut expired = Vec::new();
        for rec in &mut self.world.recommendations {
            if rec.status.is_open() && now >= rec.expires_ms {
                rec.status = RecStatus::Expired;
                rec.status_changed_ms = now;
                expired.push(rec.id.clone());
            }
        }
        for id in expired {
            self.emit(
                out,
                DomainEvent::RecommendationStatusChanged {
                    id,
                    status: RecStatus::Expired,
                    by: "runtime".to_string(),
                },
            );
        }
    }
}

fn step_toward(pos: Vec2, target: Vec2, speed: f64, dt_s: f64) -> (Vec2, Vec2, bool) {
    let delta = target.sub(pos);
    let dist = delta.len();
    let step = speed * dt_s;
    if dist <= step {
        (target, Vec2::default(), true)
    } else {
        let dir = delta.scale(1.0 / dist);
        (pos.add(dir.scale(step)), dir.scale(speed), false)
    }
}

/// Run a scenario headless to completion with a scripted operator that
/// approves every pending recommendation after a fixed think time. Used by
/// benchmark mode and the determinism tests.
pub fn run_scripted(scenario: Scenario, approve_after_ms: u64) -> (Vec<Event>, RunMetrics) {
    let mut engine = SimEngine::new(scenario);
    let mut events = engine.initial_events();
    events.extend(
        engine
            .apply(OperatorAction::Start, "scripted-operator")
            .expect("fresh engine can start"),
    );
    while engine.world().phase == SimPhase::Running {
        events.extend(engine.tick());
        let now = engine.world().sim_time_ms;
        let due: Vec<String> = engine
            .world()
            .recommendations
            .iter()
            .filter(|r| r.status == RecStatus::Pending && now >= r.created_ms + approve_after_ms)
            .map(|r| r.id.clone())
            .collect();
        for id in due {
            // The world may have moved on (asset taken, track lost); a
            // scripted operator just skips those.
            if let Ok(evs) = engine.apply(OperatorAction::Approve(id), "scripted-operator") {
                events.extend(evs);
            }
        }
    }
    let metrics = engine
        .final_metrics()
        .expect("completed run has metrics")
        .clone();
    (events, metrics)
}
