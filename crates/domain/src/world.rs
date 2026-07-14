use serde::{Deserialize, Serialize};

use crate::types::*;

/// The common operating picture. This is the single authoritative state
/// object: the runtime owns it, snapshots of it go over the wire, and the
/// browser only ever renders it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorldState {
    pub scenario_id: String,
    pub scenario_name: String,
    pub seed: u64,
    pub phase: SimPhase,
    pub sim_time_ms: u64,
    pub duration_ms: u64,
    pub bounds: Bounds,
    pub zones: Vec<Zone>,
    pub assets: Vec<Asset>,
    pub tracks: Vec<Track>,
    pub links: Vec<Link>,
    pub nodes: Vec<Node>,
    pub recommendations: Vec<Recommendation>,
    pub assignments: Vec<Assignment>,
}

impl WorldState {
    pub fn track(&self, id: &str) -> Option<&Track> {
        self.tracks.iter().find(|t| t.id == id)
    }

    pub fn asset(&self, id: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.id == id)
    }

    pub fn asset_mut(&mut self, id: &str) -> Option<&mut Asset> {
        self.assets.iter_mut().find(|a| a.id == id)
    }

    pub fn zone(&self, id: &str) -> Option<&Zone> {
        self.zones.iter().find(|z| z.id == id)
    }

    pub fn link_mut(&mut self, id: &str) -> Option<&mut Link> {
        self.links.iter_mut().find(|l| l.id == id)
    }

    pub fn recommendation(&self, id: &str) -> Option<&Recommendation> {
        self.recommendations.iter().find(|r| r.id == id)
    }

    pub fn recommendation_mut(&mut self, id: &str) -> Option<&mut Recommendation> {
        self.recommendations.iter_mut().find(|r| r.id == id)
    }

    /// Open (pending or acknowledged) recommendations.
    pub fn open_recommendations(&self) -> impl Iterator<Item = &Recommendation> {
        self.recommendations.iter().filter(|r| r.status.is_open())
    }

    pub fn active_assignments(&self) -> impl Iterator<Item = &Assignment> {
        self.assignments
            .iter()
            .filter(|a| a.status == AssignmentStatus::Active)
    }
}
