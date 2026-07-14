//! The load-bearing guarantees: identical inputs produce identical runs,
//! different seeds do not, and nothing moves without operator approval.

use std::path::{Path, PathBuf};

use dira_domain::{DomainEvent, SimPhase};
use dira_simulator::{load_scenario, run_scripted, OperatorAction, Scenario, SimEngine};

fn scenario(name: &str) -> Scenario {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scenarios")
        .join(name);
    load_scenario(Path::new(&path)).expect("scenario loads")
}

#[test]
fn same_scenario_and_seed_produce_identical_runs() {
    let (events_a, metrics_a) = run_scripted(scenario("perimeter-incursion.toml"), 2_000);
    let (events_b, metrics_b) = run_scripted(scenario("perimeter-incursion.toml"), 2_000);
    assert_eq!(events_a.len(), events_b.len());
    assert_eq!(events_a, events_b);
    assert_eq!(metrics_a, metrics_b);
}

#[test]
fn different_seed_produces_a_different_run() {
    let mut alt = scenario("perimeter-incursion.toml");
    alt.seed = 1337;
    let (events_a, _) = run_scripted(scenario("perimeter-incursion.toml"), 2_000);
    let (events_b, _) = run_scripted(alt, 2_000);
    assert_ne!(events_a, events_b);
}

#[test]
fn incursion_scenario_produces_expected_activity() {
    let (events, metrics) = run_scripted(scenario("perimeter-incursion.toml"), 2_000);
    let has = |pred: &dyn Fn(&DomainEvent) -> bool| events.iter().any(|e| pred(&e.body));

    assert!(has(&|b| matches!(b, DomainEvent::TrackAppeared { .. })));
    assert!(has(&|b| matches!(b, DomainEvent::LinkStateChanged { .. })));
    assert!(has(&|b| matches!(b, DomainEvent::RecommendationIssued { .. })));
    assert!(has(&|b| matches!(b, DomainEvent::AssignmentCreated { .. })));
    assert!(has(&|b| matches!(b, DomainEvent::ScenarioCompleted { .. })));
    // The staged uplink outage must actually cost us track custody.
    assert!(has(&|b| matches!(
        b,
        DomainEvent::TrackStatusChanged {
            status: dira_domain::TrackStatus::Lost,
            ..
        }
    )));
    assert!(metrics.incursions >= 1, "metrics: {metrics:?}");
    assert!(metrics.recommendations_issued > 0);
    assert!(metrics.degraded_link_time_pct > 0.0);
}

#[test]
fn no_operator_means_no_assignments() {
    // Scripted approval delay longer than the scenario: recommendations are
    // issued but never approved, so nothing may be tasked.
    let (events, metrics) = run_scripted(scenario("perimeter-incursion.toml"), 10_000_000);
    assert!(events
        .iter()
        .any(|e| matches!(e.body, DomainEvent::RecommendationIssued { .. })));
    assert!(!events
        .iter()
        .any(|e| matches!(e.body, DomainEvent::AssignmentCreated { .. })));
    assert_eq!(metrics.recommendations_approved, 0);
}

#[test]
fn disabled_policies_stay_silent() {
    let mut s = scenario("perimeter-incursion.toml");
    s.policy.coverage.enabled = false;
    s.policy.track_flag.enabled = false;
    s.policy.investigate.enabled = false;
    s.policy.comms.enabled = false;
    let (events, metrics) = run_scripted(s, 2_000);
    assert!(!events
        .iter()
        .any(|e| matches!(e.body, DomainEvent::RecommendationIssued { .. })));
    assert_eq!(metrics.recommendations_issued, 0);
}

#[test]
fn baseline_scenario_completes_quietly() {
    let (events, metrics) = run_scripted(scenario("perimeter-baseline.toml"), 2_000);
    assert!(events
        .iter()
        .any(|e| matches!(e.body, DomainEvent::ScenarioCompleted { .. })));
    // Nominal links throughout.
    assert_eq!(metrics.degraded_link_time_pct, 0.0);
    // Transit tracks age out of the picture after they exit; only those two
    // may ever be lost in a no-fault run.
    assert!(metrics.tracks_lost <= 2, "metrics: {metrics:?}");
}

#[test]
fn commands_fail_closed() {
    let mut engine = SimEngine::new(scenario("perimeter-baseline.toml"));
    engine.initial_events();
    // Cannot pause before starting.
    assert!(engine.apply(OperatorAction::Pause, "operator").is_err());
    // Unknown recommendation ids are rejected.
    assert!(engine
        .apply(OperatorAction::Approve("R-9999".into()), "operator")
        .is_err());
    // Valid lifecycle works.
    assert!(engine.apply(OperatorAction::Start, "operator").is_ok());
    assert_eq!(engine.world().phase, SimPhase::Running);
    assert!(engine.apply(OperatorAction::Start, "operator").is_err());
    assert!(engine.apply(OperatorAction::Pause, "operator").is_ok());
    assert!(engine.apply(OperatorAction::Resume, "operator").is_ok());
}
