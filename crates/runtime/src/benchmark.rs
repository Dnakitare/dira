//! Headless benchmark mode: run a scenario across several seeds with a
//! scripted operator and print per-run and aggregate metrics as JSON.
//! Comparing two policy configurations is two invocations with --policy.
//! `--stress N` instead synthesizes an N-track scenario and reports
//! tick-time percentiles and snapshot serialization cost.

use std::path::Path;
use std::time::Instant;

use anyhow::Result;

use dira_domain::{RunMetrics, SimPhase};
use dira_simulator::{load_scenario, run_scripted, OperatorAction, SimEngine};

pub fn run(
    scenario_path: &Path,
    policy_path: Option<&Path>,
    runs: u32,
    approve_after_ms: u64,
    base_seed: Option<u64>,
) -> Result<()> {
    let mut base = load_scenario(scenario_path)?;
    if let Some(policy) = policy_path {
        base.policy = crate::sim::load_policy_file(policy)?;
    }
    let first_seed = base_seed.unwrap_or(base.seed);

    let mut results: Vec<RunMetrics> = Vec::new();
    for i in 0..runs {
        let mut scenario = base.clone();
        scenario.seed = first_seed + i as u64;
        let seed = scenario.seed;
        let (_, metrics) = run_scripted(scenario, approve_after_ms);
        eprintln!(
            "run {}/{} seed {} coverage {:.3} incursions {}",
            i + 1,
            runs,
            seed,
            metrics.coverage_continuity,
            metrics.incursions
        );
        results.push(metrics);
    }

    let mean = |f: &dyn Fn(&RunMetrics) -> f64| -> f64 {
        results.iter().map(f).sum::<f64>() / results.len() as f64
    };
    let mean_opt = |f: &dyn Fn(&RunMetrics) -> Option<f64>| -> Option<f64> {
        let vals: Vec<f64> = results.iter().filter_map(f).collect();
        if vals.is_empty() {
            None
        } else {
            Some(vals.iter().sum::<f64>() / vals.len() as f64)
        }
    };

    let report = serde_json::json!({
        "scenario": base.id,
        "policy_file": policy_path.map(|p| p.display().to_string()),
        "runs": results,
        "aggregate": {
            "runs": results.len(),
            "coverage_continuity_mean": mean(&|m| m.coverage_continuity),
            "degraded_link_time_pct_mean": mean(&|m| m.degraded_link_time_pct),
            "incursions_mean": mean(&|m| m.incursions as f64),
            "time_to_flag_ms_mean": mean_opt(&|m| m.mean_time_to_flag_ms),
            "recommendations_issued_mean": mean(&|m| m.recommendations_issued as f64),
            "recommendations_approved_mean": mean(&|m| m.recommendations_approved as f64),
            "response_latency_ms_mean": mean_opt(&|m| m.mean_response_latency_ms),
            "tracks_lost_mean": mean(&|m| m.tracks_lost as f64),
        },
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Synthesize an n-track scenario, run it for 60 s of sim time, and report
/// tick-time percentiles plus the cost of serializing one full snapshot.
/// Policies stay enabled: policy evaluation is part of the workload.
pub fn stress(n_tracks: usize) -> Result<()> {
    let scenario = build_stress_scenario(n_tracks);
    let mut engine = SimEngine::new(scenario);
    engine.initial_events();
    engine
        .apply(OperatorAction::Start, "stress")
        .expect("fresh engine starts");

    let mut tick_ms: Vec<f64> = Vec::with_capacity(700);
    while engine.world().phase == SimPhase::Running {
        let t0 = Instant::now();
        engine.tick();
        tick_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let t0 = Instant::now();
    let snapshot = serde_json::to_string(engine.world())?;
    let serialize_ms = t0.elapsed().as_secs_f64() * 1000.0;

    tick_ms.sort_by(|a, b| a.partial_cmp(b).expect("finite"));
    let pct = |p: f64| tick_ms[((tick_ms.len() as f64 - 1.0) * p) as usize];
    let report = serde_json::json!({
        "tracks": n_tracks,
        "sim_seconds": tick_ms.len() as f64 / 10.0,
        "tick_ms": {
            "p50": pct(0.50),
            "p95": pct(0.95),
            "p99": pct(0.99),
            "max": tick_ms.last(),
            "mean": tick_ms.iter().sum::<f64>() / tick_ms.len() as f64,
        },
        "budget_ms_per_tick": 100.0,
        "world_at_end": {
            "tracks_in_picture": engine.world().tracks.len(),
            "recommendations": engine.world().recommendations.len(),
        },
        "snapshot": {
            "bytes": snapshot.len(),
            "serialize_ms": serialize_ms,
        },
    });
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Deterministic synthetic scenario: 4 protected zones, 12 patrol assets,
/// 4 links, and n wandering tracks spread over a grid.
fn build_stress_scenario(n_tracks: usize) -> dira_simulator::Scenario {
    use dira_domain::{
        Asset, AssetKind, AssetStatus, Bounds, Link, LinkState, Node, NodeHealth, PolicyConfig,
        TrackClass, Vec2, Zone, ZoneKind,
    };
    use dira_simulator::scenario::{AssetInit, Motion, Scenario, TrackPlan};

    let side = 20_000.0_f64;
    let links: Vec<Link> = (0..4)
        .map(|i| Link {
            id: format!("l-{i:02}"),
            name: format!("Sensor uplink {i:02}"),
            state: LinkState::Nominal,
            since_ms: 0,
        })
        .collect();
    let zones: Vec<Zone> = [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)]
        .iter()
        .enumerate()
        .map(|(i, (sx, sy))| Zone {
            id: format!("z-{i:02}"),
            name: format!("Zone {i:02}"),
            kind: ZoneKind::Protected,
            center: Vec2::new(sx * side / 4.0, sy * side / 4.0),
            radius_m: 1500.0,
            priority: 1,
            covered: false,
        })
        .collect();
    let assets: Vec<AssetInit> = (0..12)
        .map(|i| {
            let angle = i as f64 / 12.0 * std::f64::consts::TAU;
            let start = Vec2::new(angle.cos() * side / 3.0, angle.sin() * side / 3.0);
            AssetInit {
                asset: Asset {
                    id: format!("a-{i:02}"),
                    kind: AssetKind::Patrol,
                    pos: start,
                    vel: Vec2::default(),
                    speed_mps: 15.0,
                    observe_radius_m: 400.0,
                    status: AssetStatus::Available,
                    assignment: None,
                },
                patrol: vec![start, Vec2::new(start.x * 0.5, start.y * 0.5)],
            }
        })
        .collect();
    let cols = (n_tracks as f64).sqrt().ceil() as usize;
    let tracks: Vec<TrackPlan> = (0..n_tracks)
        .map(|i| {
            let cx = (i % cols) as f64 / cols as f64 - 0.5;
            let cy = (i / cols) as f64 / cols as f64 - 0.5;
            TrackPlan {
                id: format!("t-{i:05}"),
                class: TrackClass::Unknown,
                motion: Motion::Wander,
                enter_at_ms: 0,
                start: Vec2::new(cx * side * 0.9, cy * side * 0.9),
                speed: 10.0 + (i % 25) as f64,
                via_link: format!("l-{:02}", i % 4),
            }
        })
        .collect();

    Scenario {
        id: format!("stress-{n_tracks}"),
        name: format!("Synthetic stress: {n_tracks} tracks"),
        description: String::new(),
        seed: 42,
        duration_ms: 60_000,
        bounds: Bounds {
            width: side,
            height: side,
        },
        policy: PolicyConfig::default(),
        zones,
        assets,
        tracks,
        links,
        nodes: vec![Node {
            id: "n-edge".into(),
            name: "Edge runtime node".into(),
            health: NodeHealth::Nominal,
        }],
        faults: Vec::new(),
    }
}
