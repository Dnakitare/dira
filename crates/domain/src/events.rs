use serde::{Deserialize, Serialize};

use crate::metrics::RunMetrics;
use crate::types::*;

/// Audit event envelope. `seq` is assigned by the engine and is strictly
/// increasing within a run; `sim_time_ms` is simulation time, never wall time,
/// so the same scenario and seed always produce the same event stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    pub sim_time_ms: u64,
    #[serde(flatten)]
    pub body: DomainEvent,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DomainEvent {
    ScenarioLoaded {
        scenario_id: String,
        name: String,
        seed: u64,
    },
    PhaseChanged {
        phase: SimPhase,
    },
    TrackAppeared {
        track: TrackId,
        class: TrackClass,
    },
    TrackStatusChanged {
        track: TrackId,
        status: TrackStatus,
    },
    TrackDropped {
        track: TrackId,
    },
    ZoneCoverageChanged {
        zone: ZoneId,
        covered: bool,
    },
    LinkStateChanged {
        link: LinkId,
        state: LinkState,
    },
    NodeHealthChanged {
        node: NodeId,
        health: NodeHealth,
    },
    FaultInjected {
        by: String,
        fault: FaultSpec,
    },
    RecommendationIssued {
        recommendation: Recommendation,
    },
    RecommendationStatusChanged {
        id: String,
        status: RecStatus,
        by: String,
    },
    AssignmentCreated {
        assignment: Assignment,
    },
    AssignmentCompleted {
        id: String,
        outcome: String,
    },
    ScenarioCompleted {
        metrics: RunMetrics,
    },
}
