//! Headless benchmark mode: run a scenario across several seeds with a
//! scripted operator and print per-run and aggregate metrics as JSON.
//! Comparing two policy configurations is two invocations with --policy.

use std::path::Path;

use anyhow::Result;

use dira_domain::RunMetrics;
use dira_simulator::{load_scenario, run_scripted};

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
        results.iter().map(|m| f(m)).sum::<f64>() / results.len() as f64
    };
    let mean_opt = |f: &dyn Fn(&RunMetrics) -> Option<f64>| -> Option<f64> {
        let vals: Vec<f64> = results.iter().filter_map(|m| f(m)).collect();
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
