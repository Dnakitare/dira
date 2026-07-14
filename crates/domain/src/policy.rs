use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::types::*;
use crate::world::WorldState;

/// Configurable, transparent coordination policies. Every recommendation a
/// policy produces carries its policy id, a human-readable reason, and a
/// confidence value, and does nothing until an operator approves it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PolicyConfig {
    /// How long a recommendation stays open before expiring.
    pub recommendation_ttl_s: f64,
    pub coverage: CoverageCfg,
    pub track_flag: TrackFlagCfg,
    pub investigate: InvestigateCfg,
    pub comms: CommsCfg,
}

impl Default for PolicyConfig {
    fn default() -> Self {
        Self {
            recommendation_ttl_s: 45.0,
            coverage: CoverageCfg::default(),
            track_flag: TrackFlagCfg::default(),
            investigate: InvestigateCfg::default(),
            comms: CommsCfg::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoverageCfg {
    pub enabled: bool,
}

impl Default for CoverageCfg {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TrackFlagCfg {
    pub enabled: bool,
    /// Flag a track when its projected zone entry is within this horizon.
    pub horizon_s: f64,
}

impl Default for TrackFlagCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            horizon_s: 45.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InvestigateCfg {
    pub enabled: bool,
}

impl Default for InvestigateCfg {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CommsCfg {
    pub enabled: bool,
    /// Recommend a comms check after a link has been degraded this long.
    pub degraded_alert_s: f64,
}

impl Default for CommsCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            degraded_alert_s: 10.0,
        }
    }
}

/// Per-run policy state: dedupe keys that reached a terminal operator
/// decision, so policies do not nag about subjects the operator already
/// decided on. Episode resets (a link recovering, a zone regaining coverage)
/// clear the corresponding key.
#[derive(Debug, Default, Clone)]
pub struct PolicyMemory {
    resolved: HashSet<String>,
}

impl PolicyMemory {
    pub fn mark_resolved(&mut self, key: String) {
        self.resolved.insert(key);
    }
}

/// Evaluate all enabled policies against the current world state and return
/// new recommendations. Deterministic: iteration follows insertion order and
/// asset selection tie-breaks on id.
pub fn evaluate_policies(
    world: &WorldState,
    cfg: &PolicyConfig,
    mem: &mut PolicyMemory,
    rec_seq: &mut u64,
) -> Vec<Recommendation> {
    let now = world.sim_time_ms;
    let ttl_ms = (cfg.recommendation_ttl_s * 1000.0) as u64;

    let open_keys: HashSet<String> = world
        .open_recommendations()
        .map(|r| r.kind.dedupe_key())
        .collect();

    // Assets already referenced by an open assignment-shaped recommendation,
    // so two policies do not double-book the same asset in one tick.
    let mut booked: HashSet<AssetId> = world
        .open_recommendations()
        .filter_map(|r| match &r.kind {
            RecKind::AssignObservation { asset, .. } => Some(asset.clone()),
            RecKind::InvestigateTrack { asset, .. } => Some(asset.clone()),
            _ => None,
        })
        .collect();

    let mut out = Vec::new();
    let issue = |kind: RecKind,
                 policy_id: &str,
                 reason: String,
                 confidence: f64,
                 out: &mut Vec<Recommendation>,
                 rec_seq: &mut u64| {
        *rec_seq += 1;
        out.push(Recommendation {
            id: format!("R-{:04}", *rec_seq),
            policy_id: policy_id.to_string(),
            kind,
            reason,
            confidence,
            created_ms: now,
            status: RecStatus::Pending,
            status_changed_ms: now,
            expires_ms: now + ttl_ms,
        });
    };

    // --- coverage-v1: every protected zone should have an asset inside it.
    if cfg.coverage.enabled {
        for zone in world.zones.iter().filter(|z| z.kind == ZoneKind::Protected) {
            let key = format!("coverage:{}", zone.id);
            if zone.covered {
                mem.resolved.remove(&key);
                continue;
            }
            if open_keys.contains(&key) || mem.resolved.contains(&key) {
                continue;
            }
            if let Some(asset) = nearest_available(world, zone.center, &booked) {
                let dist = asset.pos.dist(zone.center);
                booked.insert(asset.id.clone());
                issue(
                    RecKind::AssignObservation {
                        asset: asset.id.clone(),
                        zone: zone.id.clone(),
                    },
                    "coverage-v1",
                    format!(
                        "Zone {} has no coverage; {} is the nearest available asset ({:.0} m away)",
                        zone.name, asset.id, dist
                    ),
                    0.9,
                    &mut out,
                    rec_seq,
                );
            }
        }
    }

    // --- track-flag-v1: bring tracks inside or approaching a protected zone
    // to operator attention.
    if cfg.track_flag.enabled {
        for track in world
            .tracks
            .iter()
            .filter(|t| t.status != TrackStatus::Lost && !t.flagged)
        {
            for zone in world.zones.iter().filter(|z| z.kind == ZoneKind::Protected) {
                let key = format!("flag:{}:{}", track.id, zone.id);
                if open_keys.contains(&key) || mem.resolved.contains(&key) {
                    continue;
                }
                let dist = track.pos.dist(zone.center);
                if dist <= zone.radius_m {
                    issue(
                        RecKind::FlagTrack {
                            track: track.id.clone(),
                            zone: zone.id.clone(),
                        },
                        "track-flag-v1",
                        format!("Track {} is inside protected zone {}", track.id, zone.name),
                        0.95,
                        &mut out,
                        rec_seq,
                    );
                } else if let Some(t_entry) = time_to_entry(track, zone) {
                    if t_entry <= cfg.track_flag.horizon_s {
                        let confidence = 0.5 + 0.45 * (1.0 - t_entry / cfg.track_flag.horizon_s);
                        issue(
                            RecKind::FlagTrack {
                                track: track.id.clone(),
                                zone: zone.id.clone(),
                            },
                            "track-flag-v1",
                            format!(
                                "Track {} projected to enter zone {} in ~{:.0} s",
                                track.id, zone.name, t_entry
                            ),
                            confidence,
                            &mut out,
                            rec_seq,
                        );
                    }
                }
            }
        }
    }

    // --- investigate-v1: send an available asset to look at a flagged track.
    if cfg.investigate.enabled {
        let under_investigation: HashSet<&str> = world
            .active_assignments()
            .filter_map(|a| match &a.objective {
                Objective::InvestigateTrack { track } => Some(track.as_str()),
                _ => None,
            })
            .collect();
        for track in world
            .tracks
            .iter()
            .filter(|t| t.flagged && t.status != TrackStatus::Lost)
        {
            let key = format!("investigate:{}", track.id);
            if open_keys.contains(&key)
                || mem.resolved.contains(&key)
                || under_investigation.contains(track.id.as_str())
            {
                continue;
            }
            if let Some(asset) = nearest_available(world, track.pos, &booked) {
                booked.insert(asset.id.clone());
                issue(
                    RecKind::InvestigateTrack {
                        asset: asset.id.clone(),
                        track: track.id.clone(),
                    },
                    "investigate-v1",
                    format!(
                        "Flagged track {} is unobserved; {} can intercept its last known position",
                        track.id, asset.id
                    ),
                    0.8,
                    &mut out,
                    rec_seq,
                );
            }
        }
    }

    // --- comms-v1: a link degraded past the alert threshold warrants a check.
    if cfg.comms.enabled {
        let alert_ms = (cfg.comms.degraded_alert_s * 1000.0) as u64;
        for link in &world.links {
            let key = format!("comms:{}", link.id);
            if link.state.is_nominal() {
                mem.resolved.remove(&key);
                continue;
            }
            if open_keys.contains(&key) || mem.resolved.contains(&key) {
                continue;
            }
            if now.saturating_sub(link.since_ms) >= alert_ms {
                issue(
                    RecKind::CommsCheck {
                        link: link.id.clone(),
                    },
                    "comms-v1",
                    format!(
                        "Link {} has been {} for {:.0} s; tracks on this link are degraded",
                        link.name,
                        describe_link_state(&link.state),
                        (now - link.since_ms) as f64 / 1000.0
                    ),
                    0.85,
                    &mut out,
                    rec_seq,
                );
            }
        }
    }

    out
}

fn nearest_available<'a>(
    world: &'a WorldState,
    to: Vec2,
    booked: &HashSet<AssetId>,
) -> Option<&'a Asset> {
    world
        .assets
        .iter()
        .filter(|a| a.status == AssetStatus::Available && !booked.contains(&a.id))
        .min_by(|a, b| {
            let da = a.pos.dist(to);
            let db = b.pos.dist(to);
            da.partial_cmp(&db)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        })
}

/// Seconds until the track's straight-line projection crosses the zone
/// boundary, if it is closing.
fn time_to_entry(track: &Track, zone: &Zone) -> Option<f64> {
    let rel = track.pos.sub(zone.center);
    let dist = rel.len();
    if dist <= zone.radius_m {
        return Some(0.0);
    }
    // Rate at which the distance to center is shrinking.
    let closing = -rel.norm().dot(track.vel);
    if closing > 0.5 {
        Some((dist - zone.radius_m) / closing)
    } else {
        None
    }
}

pub fn describe_link_state(state: &LinkState) -> String {
    match state {
        LinkState::Nominal => "nominal".to_string(),
        LinkState::Delayed { delay_ms } => format!("delayed ({} ms)", delay_ms),
        LinkState::Intermittent { loss } => {
            format!("intermittent ({:.0}% loss)", loss * 100.0)
        }
        LinkState::Unavailable => "unavailable".to_string(),
    }
}
