use serde::{Deserialize, Serialize};

pub type TrackId = String;
pub type AssetId = String;
pub type ZoneId = String;
pub type LinkId = String;
pub type NodeId = String;

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn dist(&self, other: Vec2) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }

    pub fn len(&self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn sub(&self, other: Vec2) -> Vec2 {
        Vec2::new(self.x - other.x, self.y - other.y)
    }

    pub fn add(&self, other: Vec2) -> Vec2 {
        Vec2::new(self.x + other.x, self.y + other.y)
    }

    pub fn scale(&self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }

    pub fn dot(&self, other: Vec2) -> f64 {
        self.x * other.x + self.y * other.y
    }

    /// Unit vector, or zero if the vector has no length.
    pub fn norm(&self) -> Vec2 {
        let l = self.len();
        if l > 1e-9 {
            self.scale(1.0 / l)
        } else {
            Vec2::default()
        }
    }
}

/// World extent in meters, centered on the origin.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub width: f64,
    pub height: f64,
}

impl Bounds {
    pub fn contains(&self, p: Vec2) -> bool {
        p.x.abs() <= self.width / 2.0 && p.y.abs() <= self.height / 2.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimPhase {
    Idle,
    Running,
    Paused,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackClass {
    Unknown,
    Ground,
    Air,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrackStatus {
    Active,
    Stale,
    Lost,
}

/// An observed moving entity in the common operating picture. Position and
/// velocity are as *observed* through a link, not ground truth.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    pub class: TrackClass,
    pub pos: Vec2,
    pub vel: Vec2,
    pub status: TrackStatus,
    /// Position uncertainty radius in meters. Grows with observation staleness.
    pub uncertainty_m: f64,
    pub last_seen_ms: u64,
    pub via_link: LinkId,
    /// Set by an operator-approved flag recommendation.
    pub flagged: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Patrol,
    Observer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Available,
    Enroute,
    Observing,
    Investigating,
    Unavailable,
}

/// A controllable friendly unit. Asset telemetry is direct (not link-mediated)
/// in the MVP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Asset {
    pub id: AssetId,
    pub kind: AssetKind,
    pub pos: Vec2,
    pub vel: Vec2,
    pub speed_mps: f64,
    pub observe_radius_m: f64,
    pub status: AssetStatus,
    pub assignment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    Protected,
    Observation,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Zone {
    pub id: ZoneId,
    pub name: String,
    pub kind: ZoneKind,
    pub center: Vec2,
    pub radius_m: f64,
    pub priority: u8,
    /// Recomputed every tick: at least one asset is inside the zone.
    pub covered: bool,
}

impl Zone {
    pub fn contains(&self, p: Vec2) -> bool {
        self.center.dist(p) <= self.radius_m
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LinkState {
    Nominal,
    Delayed { delay_ms: u64 },
    Intermittent { loss: f64 },
    Unavailable,
}

impl LinkState {
    pub fn is_nominal(&self) -> bool {
        matches!(self, LinkState::Nominal)
    }
}

/// A data path between an input source and the runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub id: LinkId,
    pub name: String,
    pub state: LinkState,
    /// Sim time at which the link entered its current state.
    pub since_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeHealth {
    Nominal,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub name: String,
    pub health: NodeHealth,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecStatus {
    Pending,
    Acknowledged,
    Approved,
    Declined,
    Expired,
}

impl RecStatus {
    /// Still awaiting a terminal operator decision.
    pub fn is_open(&self) -> bool {
        matches!(self, RecStatus::Pending | RecStatus::Acknowledged)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RecKind {
    /// Assign an available asset to cover an uncovered zone.
    AssignObservation { asset: AssetId, zone: ZoneId },
    /// Send an available asset to investigate a flagged track.
    InvestigateTrack { asset: AssetId, track: TrackId },
    /// Bring a track to operator attention (entering or approaching a zone).
    FlagTrack { track: TrackId, zone: ZoneId },
    /// A link has been degraded long enough to warrant a check.
    CommsCheck { link: LinkId },
}

impl RecKind {
    /// Stable key used to avoid issuing duplicate recommendations for the
    /// same subject.
    pub fn dedupe_key(&self) -> String {
        match self {
            RecKind::AssignObservation { zone, .. } => format!("coverage:{zone}"),
            RecKind::InvestigateTrack { track, .. } => format!("investigate:{track}"),
            RecKind::FlagTrack { track, zone } => format!("flag:{track}:{zone}"),
            RecKind::CommsCheck { link } => format!("comms:{link}"),
        }
    }
}

/// A runtime-generated suggestion. Never self-executing: state only changes
/// through an explicit operator approval event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recommendation {
    pub id: String,
    pub policy_id: String,
    pub kind: RecKind,
    pub reason: String,
    pub confidence: f64,
    pub created_ms: u64,
    pub status: RecStatus,
    pub status_changed_ms: u64,
    pub expires_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Objective {
    ObserveZone { zone: ZoneId },
    InvestigateTrack { track: TrackId },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Active,
    Completed,
    Aborted,
}

/// The result of an approved recommendation: a concrete objective for one asset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assignment {
    pub id: String,
    pub asset: AssetId,
    pub objective: Objective,
    pub created_ms: u64,
    pub status: AssignmentStatus,
    pub from_recommendation: String,
}

/// Presentation-only georeference for a scenario: a pre-rendered map texture
/// draped under the scene. Never consulted by simulation or policy logic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Basemap {
    /// URL path relative to the served web root, e.g. "basemaps/coos-bay.png".
    pub url: String,
    /// Square extent covered by the texture, in meters, centered on origin.
    pub extent_m: f64,
    /// [lat, lon] of the scene origin, for provenance and future adapters.
    pub origin: [f64; 2],
}

/// A condition change that can come from a scenario timeline or an
/// operator-approved injection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum FaultSpec {
    Link { link: LinkId, state: LinkState },
    Node { node: NodeId, health: NodeHealth },
}
