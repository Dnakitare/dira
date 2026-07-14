//! Property-based checks over randomly generated scenarios. The point is to
//! enforce the invariants across the input space, not just the shipped
//! scenario files: determinism, the operator gate, and audit-log ordering.

use proptest::prelude::*;

use dira_domain::{
    Asset, AssetKind, AssetStatus, Bounds, DomainEvent, Link, LinkState, Node, NodeHealth,
    PolicyConfig, TrackClass, Vec2, Zone, ZoneKind,
};
use dira_simulator::run_scripted;
use dira_simulator::scenario::{AssetInit, Motion, Scenario, TimedFault, TrackPlan};

const SIDE: f64 = 6000.0;

/// Deterministically expand a small tuple of primitives into a valid scenario.
fn build_scenario(
    seed: u64,
    n_tracks: usize,
    n_assets: usize,
    n_zones: usize,
    faults: Vec<(u8, u8)>,
) -> Scenario {
    let links: Vec<Link> = (0..2)
        .map(|i| Link {
            id: format!("l-{i}"),
            name: format!("link {i}"),
            state: LinkState::Nominal,
            since_ms: 0,
        })
        .collect();
    let zones: Vec<Zone> = (0..n_zones)
        .map(|i| Zone {
            id: format!("z-{i}"),
            name: format!("zone {i}"),
            kind: ZoneKind::Protected,
            center: Vec2::new(
                (i as f64 - 0.5) * SIDE / 3.0,
                ((i % 2) as f64 - 0.5) * SIDE / 3.0,
            ),
            radius_m: 500.0,
            priority: 1,
            covered: false,
        })
        .collect();
    let assets: Vec<AssetInit> = (0..n_assets)
        .map(|i| {
            let start = Vec2::new((i as f64 + 1.0) * 300.0 - SIDE / 4.0, 0.0);
            AssetInit {
                asset: Asset {
                    id: format!("a-{i}"),
                    kind: AssetKind::Patrol,
                    pos: start,
                    vel: Vec2::default(),
                    speed_mps: 12.0,
                    observe_radius_m: 250.0,
                    status: AssetStatus::Available,
                    assignment: None,
                },
                patrol: vec![start, Vec2::new(start.x, start.y + 900.0)],
            }
        })
        .collect();
    let tracks: Vec<TrackPlan> = (0..n_tracks)
        .map(|i| {
            let start = Vec2::new((i as f64 / n_tracks as f64 - 0.5) * SIDE * 0.8, SIDE * 0.35);
            let motion = match i % 3 {
                0 => Motion::Wander,
                1 => Motion::Transit {
                    exit: Vec2::new(-start.x, -SIDE * 0.4),
                },
                _ => Motion::Incursion {
                    zone: format!("z-{}", i % n_zones),
                },
            };
            TrackPlan {
                id: format!("t-{i}"),
                class: TrackClass::Unknown,
                motion,
                enter_at_ms: (i as u64 % 5) * 2000,
                start,
                speed: 15.0 + (i % 10) as f64 * 3.0,
                via_link: format!("l-{}", i % 2),
            }
        })
        .collect();
    let faults: Vec<TimedFault> = faults
        .into_iter()
        .map(|(at, kind)| {
            let state = match kind % 4 {
                0 => LinkState::Nominal,
                1 => LinkState::Delayed { delay_ms: 1200 },
                2 => LinkState::Intermittent { loss: 0.5 },
                _ => LinkState::Unavailable,
            };
            TimedFault {
                at_ms: at as u64 * 250,
                fault: dira_domain::FaultSpec::Link {
                    link: format!("l-{}", kind % 2),
                    state,
                },
            }
        })
        .collect();
    let mut faults = faults;
    faults.sort_by_key(|f| f.at_ms);

    Scenario {
        id: "prop".into(),
        name: "generated".into(),
        description: String::new(),
        seed,
        duration_ms: 30_000,
        bounds: Bounds {
            width: SIDE,
            height: SIDE,
        },
        policy: PolicyConfig::default(),
        zones,
        assets,
        tracks,
        links,
        nodes: vec![Node {
            id: "n-0".into(),
            name: "node".into(),
            health: NodeHealth::Nominal,
        }],
        faults,
    }
}

fn params() -> impl Strategy<Value = (u64, usize, usize, usize, Vec<(u8, u8)>)> {
    (
        0u64..u64::MAX / 2,
        1usize..8,
        1usize..5,
        1usize..3,
        prop::collection::vec((0u8..120, 0u8..8), 0..5),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(24))]

    #[test]
    fn determinism_holds_for_generated_scenarios(
        (seed, n_tracks, n_assets, n_zones, faults) in params()
    ) {
        let a = run_scripted(
            build_scenario(seed, n_tracks, n_assets, n_zones, faults.clone()),
            1500,
        );
        let b = run_scripted(build_scenario(seed, n_tracks, n_assets, n_zones, faults), 1500);
        prop_assert_eq!(&a.0, &b.0, "event streams diverged");
        prop_assert_eq!(a.1, b.1, "metrics diverged");
    }

    #[test]
    fn nothing_is_tasked_without_operator_approval(
        (seed, n_tracks, n_assets, n_zones, faults) in params()
    ) {
        // Approval delay longer than the run: recommendations may issue,
        // assignments must never exist.
        let (events, metrics) = run_scripted(
            build_scenario(seed, n_tracks, n_assets, n_zones, faults),
            10_000_000,
        );
        let any_assignment = events
            .iter()
            .any(|e| matches!(e.body, DomainEvent::AssignmentCreated { .. }));
        prop_assert!(!any_assignment, "assignment created without approval");
        prop_assert_eq!(metrics.recommendations_approved, 0);
    }

    #[test]
    fn audit_log_is_strictly_ordered(
        (seed, n_tracks, n_assets, n_zones, faults) in params()
    ) {
        let (events, _) = run_scripted(
            build_scenario(seed, n_tracks, n_assets, n_zones, faults),
            1500,
        );
        for pair in events.windows(2) {
            prop_assert!(pair[1].seq == pair[0].seq + 1, "seq must be contiguous");
            prop_assert!(
                pair[1].sim_time_ms >= pair[0].sim_time_ms,
                "sim time must be non-decreasing"
            );
        }
    }
}
