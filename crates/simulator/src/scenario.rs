use std::collections::HashSet;
use std::path::Path;

use serde::Deserialize;
use thiserror::Error;

use dira_domain::{
    Asset, AssetKind, AssetStatus, Bounds, FaultSpec, Link, LinkState, Node, NodeHealth,
    PolicyConfig, TrackClass, Vec2, Zone, ZoneKind,
};

#[derive(Debug, Error)]
pub enum ScenarioError {
    #[error("failed to read scenario file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse scenario file: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid scenario: {0}")]
    Invalid(String),
}

fn invalid(msg: impl Into<String>) -> ScenarioError {
    ScenarioError::Invalid(msg.into())
}

// ---------------------------------------------------------------------------
// Raw TOML shape. Deliberately flat and stringly-typed for readable scenario
// files; `validate` turns it into typed domain structures and rejects
// anything ambiguous.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ScenarioFile {
    pub scenario: MetaSpec,
    #[serde(default)]
    pub policy: PolicyConfig,
    #[serde(default)]
    pub zones: Vec<ZoneSpec>,
    #[serde(default)]
    pub assets: Vec<AssetSpec>,
    #[serde(default)]
    pub tracks: Vec<TrackSpec>,
    #[serde(default)]
    pub links: Vec<LinkSpec>,
    #[serde(default)]
    pub nodes: Vec<NodeSpec>,
    #[serde(default)]
    pub faults: Vec<FaultSpecRaw>,
}

#[derive(Debug, Deserialize)]
pub struct MetaSpec {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub seed: u64,
    pub duration_s: f64,
    /// [width, height] in meters, centered on the origin.
    pub bounds: [f64; 2],
}

#[derive(Debug, Deserialize)]
pub struct ZoneSpec {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub center: [f64; 2],
    pub radius: f64,
    #[serde(default = "default_priority")]
    pub priority: u8,
}

fn default_priority() -> u8 {
    1
}

#[derive(Debug, Deserialize)]
pub struct AssetSpec {
    pub id: String,
    #[serde(default = "default_asset_kind")]
    pub kind: String,
    pub start: [f64; 2],
    pub speed: f64,
    #[serde(default = "default_observe_radius")]
    pub observe_radius: f64,
    #[serde(default)]
    pub patrol: Vec<[f64; 2]>,
}

fn default_asset_kind() -> String {
    "patrol".to_string()
}

fn default_observe_radius() -> f64 {
    250.0
}

#[derive(Debug, Deserialize)]
pub struct TrackSpec {
    pub id: String,
    #[serde(default = "default_track_class")]
    pub class: String,
    /// "patrol" | "transit" | "incursion" | "wander"
    pub pattern: String,
    #[serde(default)]
    pub enter_at_s: f64,
    pub start: [f64; 2],
    pub speed: f64,
    pub via_link: String,
    #[serde(default)]
    pub target_zone: Option<String>,
    #[serde(default)]
    pub exit: Option<[f64; 2]>,
    #[serde(default)]
    pub waypoints: Vec<[f64; 2]>,
}

fn default_track_class() -> String {
    "unknown".to_string()
}

#[derive(Debug, Deserialize)]
pub struct LinkSpec {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct NodeSpec {
    pub id: String,
    pub name: String,
}

/// One timeline entry. Exactly one of `link`/`node` must be set, with the
/// matching state fields.
#[derive(Debug, Deserialize)]
pub struct FaultSpecRaw {
    pub at_s: f64,
    #[serde(default)]
    pub link: Option<String>,
    /// "nominal" | "delayed" | "intermittent" | "unavailable"
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub delay_ms: Option<u64>,
    #[serde(default)]
    pub loss: Option<f64>,
    #[serde(default)]
    pub node: Option<String>,
    /// "nominal" | "degraded" | "unavailable"
    #[serde(default)]
    pub health: Option<String>,
}

// ---------------------------------------------------------------------------
// Validated scenario
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct Scenario {
    pub id: String,
    pub name: String,
    pub description: String,
    pub seed: u64,
    pub duration_ms: u64,
    pub bounds: Bounds,
    pub policy: PolicyConfig,
    pub zones: Vec<Zone>,
    pub assets: Vec<AssetInit>,
    pub tracks: Vec<TrackPlan>,
    pub links: Vec<Link>,
    pub nodes: Vec<Node>,
    /// Sorted by time.
    pub faults: Vec<TimedFault>,
}

#[derive(Debug, Clone)]
pub struct AssetInit {
    pub asset: Asset,
    pub patrol: Vec<Vec2>,
}

#[derive(Debug, Clone)]
pub struct TrackPlan {
    pub id: String,
    pub class: TrackClass,
    pub motion: Motion,
    pub enter_at_ms: u64,
    pub start: Vec2,
    pub speed: f64,
    pub via_link: String,
}

#[derive(Debug, Clone)]
pub enum Motion {
    /// Loop over waypoints forever.
    Patrol { waypoints: Vec<Vec2> },
    /// Straight to an exit point, then gone.
    Transit { exit: Vec2 },
    /// Head for a zone, then orbit inside it.
    Incursion { zone: String },
    /// Seeded random walk within bounds.
    Wander,
}

#[derive(Debug, Clone)]
pub struct TimedFault {
    pub at_ms: u64,
    pub fault: FaultSpec,
}

pub fn load_scenario(path: &Path) -> Result<Scenario, ScenarioError> {
    let text = std::fs::read_to_string(path)?;
    let file: ScenarioFile = toml::from_str(&text)?;
    validate(file)
}

fn v2(p: [f64; 2]) -> Vec2 {
    Vec2::new(p[0], p[1])
}

pub fn validate(file: ScenarioFile) -> Result<Scenario, ScenarioError> {
    let meta = file.scenario;
    if meta.duration_s <= 0.0 {
        return Err(invalid("duration_s must be positive"));
    }
    if meta.bounds[0] <= 0.0 || meta.bounds[1] <= 0.0 {
        return Err(invalid("bounds must be positive"));
    }

    let mut ids = HashSet::new();
    for id in file
        .zones
        .iter()
        .map(|z| &z.id)
        .chain(file.assets.iter().map(|a| &a.id))
        .chain(file.tracks.iter().map(|t| &t.id))
        .chain(file.links.iter().map(|l| &l.id))
        .chain(file.nodes.iter().map(|n| &n.id))
    {
        if !ids.insert(id.clone()) {
            return Err(invalid(format!("duplicate id: {id}")));
        }
    }

    let zones: Vec<Zone> = file
        .zones
        .iter()
        .map(|z| {
            let kind = match z.kind.as_str() {
                "protected" => Ok(ZoneKind::Protected),
                "observation" => Ok(ZoneKind::Observation),
                other => Err(invalid(format!("zone {}: unknown kind '{other}'", z.id))),
            }?;
            if z.radius <= 0.0 {
                return Err(invalid(format!("zone {}: radius must be positive", z.id)));
            }
            Ok(Zone {
                id: z.id.clone(),
                name: z.name.clone(),
                kind,
                center: v2(z.center),
                radius_m: z.radius,
                priority: z.priority,
                covered: false,
            })
        })
        .collect::<Result<_, ScenarioError>>()?;

    let assets: Vec<AssetInit> = file
        .assets
        .iter()
        .map(|a| {
            let kind = match a.kind.as_str() {
                "patrol" => Ok(AssetKind::Patrol),
                "observer" => Ok(AssetKind::Observer),
                other => Err(invalid(format!("asset {}: unknown kind '{other}'", a.id))),
            }?;
            if a.speed <= 0.0 {
                return Err(invalid(format!("asset {}: speed must be positive", a.id)));
            }
            Ok(AssetInit {
                asset: Asset {
                    id: a.id.clone(),
                    kind,
                    pos: v2(a.start),
                    vel: Vec2::default(),
                    speed_mps: a.speed,
                    observe_radius_m: a.observe_radius,
                    status: AssetStatus::Available,
                    assignment: None,
                },
                patrol: a.patrol.iter().copied().map(v2).collect(),
            })
        })
        .collect::<Result<_, ScenarioError>>()?;

    let links: Vec<Link> = file
        .links
        .iter()
        .map(|l| Link {
            id: l.id.clone(),
            name: l.name.clone(),
            state: LinkState::Nominal,
            since_ms: 0,
        })
        .collect();

    let nodes: Vec<Node> = file
        .nodes
        .iter()
        .map(|n| Node {
            id: n.id.clone(),
            name: n.name.clone(),
            health: NodeHealth::Nominal,
        })
        .collect();

    let zone_ids: HashSet<&str> = zones.iter().map(|z| z.id.as_str()).collect();
    let link_ids: HashSet<&str> = links.iter().map(|l| l.id.as_str()).collect();
    let node_ids: HashSet<&str> = nodes.iter().map(|n| n.id.as_str()).collect();

    let tracks: Vec<TrackPlan> = file
        .tracks
        .iter()
        .map(|t| {
            if !link_ids.contains(t.via_link.as_str()) {
                return Err(invalid(format!(
                    "track {}: via_link '{}' does not exist",
                    t.id, t.via_link
                )));
            }
            if t.speed <= 0.0 {
                return Err(invalid(format!("track {}: speed must be positive", t.id)));
            }
            let class = match t.class.as_str() {
                "unknown" => Ok(TrackClass::Unknown),
                "ground" => Ok(TrackClass::Ground),
                "air" => Ok(TrackClass::Air),
                other => Err(invalid(format!("track {}: unknown class '{other}'", t.id))),
            }?;
            let motion = match t.pattern.as_str() {
                "patrol" => {
                    if t.waypoints.is_empty() {
                        return Err(invalid(format!(
                            "track {}: patrol pattern needs waypoints",
                            t.id
                        )));
                    }
                    Motion::Patrol {
                        waypoints: t.waypoints.iter().copied().map(v2).collect(),
                    }
                }
                "transit" => {
                    let exit = t.exit.ok_or_else(|| {
                        invalid(format!("track {}: transit pattern needs exit", t.id))
                    })?;
                    Motion::Transit { exit: v2(exit) }
                }
                "incursion" => {
                    let zone = t.target_zone.clone().ok_or_else(|| {
                        invalid(format!(
                            "track {}: incursion pattern needs target_zone",
                            t.id
                        ))
                    })?;
                    if !zone_ids.contains(zone.as_str()) {
                        return Err(invalid(format!(
                            "track {}: target_zone '{zone}' does not exist",
                            t.id
                        )));
                    }
                    Motion::Incursion { zone }
                }
                "wander" => Motion::Wander,
                other => {
                    return Err(invalid(format!(
                        "track {}: unknown pattern '{other}'",
                        t.id
                    )))
                }
            };
            Ok(TrackPlan {
                id: t.id.clone(),
                class,
                motion,
                enter_at_ms: (t.enter_at_s * 1000.0) as u64,
                start: v2(t.start),
                speed: t.speed,
                via_link: t.via_link.clone(),
            })
        })
        .collect::<Result<_, ScenarioError>>()?;

    let mut faults: Vec<TimedFault> =
        file.faults
            .iter()
            .map(|f| {
                let fault = match (&f.link, &f.node) {
                    (Some(link), None) => {
                        if !link_ids.contains(link.as_str()) {
                            return Err(invalid(format!("fault: link '{link}' does not exist")));
                        }
                        let state = f.state.as_deref().ok_or_else(|| {
                            invalid(format!("fault on link {link}: missing state"))
                        })?;
                        let state = parse_link_state(state, f.delay_ms, f.loss)?;
                        FaultSpec::Link {
                            link: link.clone(),
                            state,
                        }
                    }
                    (None, Some(node)) => {
                        if !node_ids.contains(node.as_str()) {
                            return Err(invalid(format!("fault: node '{node}' does not exist")));
                        }
                        let health = match f.health.as_deref() {
                            Some("nominal") => NodeHealth::Nominal,
                            Some("degraded") => NodeHealth::Degraded,
                            Some("unavailable") => NodeHealth::Unavailable,
                            other => {
                                return Err(invalid(format!(
                                    "fault on node {node}: bad health '{other:?}'"
                                )))
                            }
                        };
                        FaultSpec::Node {
                            node: node.clone(),
                            health,
                        }
                    }
                    _ => {
                        return Err(invalid(
                            "each fault must target exactly one of 'link' or 'node'",
                        ))
                    }
                };
                Ok(TimedFault {
                    at_ms: (f.at_s * 1000.0) as u64,
                    fault,
                })
            })
            .collect::<Result<_, ScenarioError>>()?;
    faults.sort_by_key(|f| f.at_ms);

    Ok(Scenario {
        id: meta.id,
        name: meta.name,
        description: meta.description,
        seed: meta.seed,
        duration_ms: (meta.duration_s * 1000.0) as u64,
        bounds: Bounds {
            width: meta.bounds[0],
            height: meta.bounds[1],
        },
        policy: file.policy,
        zones,
        assets,
        tracks,
        links,
        nodes,
        faults,
    })
}

pub fn parse_link_state(
    state: &str,
    delay_ms: Option<u64>,
    loss: Option<f64>,
) -> Result<LinkState, ScenarioError> {
    match state {
        "nominal" => Ok(LinkState::Nominal),
        "delayed" => {
            let delay_ms = delay_ms.ok_or_else(|| invalid("delayed link state needs delay_ms"))?;
            Ok(LinkState::Delayed { delay_ms })
        }
        "intermittent" => {
            let loss = loss.ok_or_else(|| invalid("intermittent link state needs loss"))?;
            if !(0.0..=1.0).contains(&loss) {
                return Err(invalid("loss must be between 0 and 1"));
            }
            Ok(LinkState::Intermittent { loss })
        }
        "unavailable" => Ok(LinkState::Unavailable),
        other => Err(invalid(format!("unknown link state '{other}'"))),
    }
}
