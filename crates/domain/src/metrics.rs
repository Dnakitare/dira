use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::events::{DomainEvent, Event};
use crate::types::*;
use crate::world::WorldState;

/// Aggregate outcome measures for one run. Deterministic for a given
/// scenario and seed, so two policy configurations can be compared honestly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunMetrics {
    pub scenario_id: String,
    pub seed: u64,
    pub duration_ms: u64,
    /// Fraction of protected-zone time with at least one asset present.
    pub coverage_continuity: f64,
    /// Fraction of link time spent in a non-nominal state.
    pub degraded_link_time_pct: f64,
    /// Tracks that entered a protected zone at least once.
    pub incursions: u32,
    /// Mean ms from a track first appearing to its first flag recommendation.
    pub mean_time_to_flag_ms: Option<f64>,
    pub recommendations_issued: u32,
    pub recommendations_approved: u32,
    /// Mean ms from a recommendation being issued to its first operator response.
    pub mean_response_latency_ms: Option<f64>,
    pub tracks_lost: u32,
}

#[derive(Debug, Default, Clone)]
pub struct MetricsAccumulator {
    protected_time_ms: u64,
    covered_time_ms: u64,
    link_time_ms: u64,
    degraded_time_ms: u64,
    track_appeared_ms: BTreeMap<TrackId, u64>,
    first_flag_ms: BTreeMap<TrackId, u64>,
    incursion_tracks: BTreeSet<TrackId>,
    rec_created_ms: BTreeMap<String, u64>,
    rec_first_response_ms: BTreeMap<String, u64>,
    recommendations_issued: u32,
    recommendations_approved: u32,
    lost_tracks: BTreeSet<TrackId>,
}

impl MetricsAccumulator {
    pub fn on_tick(&mut self, world: &WorldState, tick_ms: u64) {
        for zone in world.zones.iter().filter(|z| z.kind == ZoneKind::Protected) {
            self.protected_time_ms += tick_ms;
            if zone.covered {
                self.covered_time_ms += tick_ms;
            }
            for track in &world.tracks {
                if track.status != TrackStatus::Lost && zone.contains(track.pos) {
                    self.incursion_tracks.insert(track.id.clone());
                }
            }
        }
        for link in &world.links {
            self.link_time_ms += tick_ms;
            if !link.state.is_nominal() {
                self.degraded_time_ms += tick_ms;
            }
        }
    }

    pub fn on_event(&mut self, event: &Event) {
        match &event.body {
            DomainEvent::TrackAppeared { track, .. } => {
                self.track_appeared_ms
                    .entry(track.clone())
                    .or_insert(event.sim_time_ms);
            }
            DomainEvent::TrackStatusChanged { track, status } => {
                if *status == TrackStatus::Lost {
                    self.lost_tracks.insert(track.clone());
                }
            }
            DomainEvent::RecommendationIssued { recommendation } => {
                self.recommendations_issued += 1;
                self.rec_created_ms
                    .insert(recommendation.id.clone(), event.sim_time_ms);
                if let RecKind::FlagTrack { track, .. } = &recommendation.kind {
                    self.first_flag_ms
                        .entry(track.clone())
                        .or_insert(event.sim_time_ms);
                }
            }
            DomainEvent::RecommendationStatusChanged { id, status, .. } => {
                match status {
                    RecStatus::Acknowledged | RecStatus::Approved | RecStatus::Declined => {
                        self.rec_first_response_ms
                            .entry(id.clone())
                            .or_insert(event.sim_time_ms);
                    }
                    _ => {}
                }
                if *status == RecStatus::Approved {
                    self.recommendations_approved += 1;
                }
            }
            _ => {}
        }
    }

    pub fn finish(&self, world: &WorldState) -> RunMetrics {
        let flag_latencies: Vec<f64> = self
            .first_flag_ms
            .iter()
            .filter_map(|(track, flag_ms)| {
                self.track_appeared_ms
                    .get(track)
                    .map(|seen| (*flag_ms - *seen) as f64)
            })
            .collect();
        let response_latencies: Vec<f64> = self
            .rec_first_response_ms
            .iter()
            .filter_map(|(id, resp_ms)| {
                self.rec_created_ms
                    .get(id)
                    .map(|created| (*resp_ms - *created) as f64)
            })
            .collect();

        RunMetrics {
            scenario_id: world.scenario_id.clone(),
            seed: world.seed,
            duration_ms: world.sim_time_ms,
            coverage_continuity: ratio(self.covered_time_ms, self.protected_time_ms),
            degraded_link_time_pct: ratio(self.degraded_time_ms, self.link_time_ms),
            incursions: self.incursion_tracks.len() as u32,
            mean_time_to_flag_ms: mean(&flag_latencies),
            recommendations_issued: self.recommendations_issued,
            recommendations_approved: self.recommendations_approved,
            mean_response_latency_ms: mean(&response_latencies),
            tracks_lost: self.lost_tracks.len() as u32,
        }
    }
}

fn ratio(num: u64, den: u64) -> f64 {
    if den == 0 {
        1.0
    } else {
        num as f64 / den as f64
    }
}

fn mean(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        None
    } else {
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}
