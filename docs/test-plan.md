# Test plan

## Automated (cargo test --workspace)

Integration tests in `crates/simulator/tests/determinism.rs` cover the properties the project claims:

| test | property |
|------|----------|
| `same_scenario_and_seed_produce_identical_runs` | full event stream and metrics are bit-identical across runs |
| `different_seed_produces_a_different_run` | the seed actually matters |
| `incursion_scenario_produces_expected_activity` | tracks appear, links degrade, recommendations issue, assignments happen, custody is lost during the outage, the run completes with metrics |
| `no_operator_means_no_assignments` | with no approvals, recommendations issue but nothing is ever tasked |
| `disabled_policies_stay_silent` | policy config is honored; zero recommendations |
| `baseline_scenario_completes_quietly` | no-fault control run: zero degraded link time |
| `commands_fail_closed` | wrong-phase and unknown-id commands are rejected; valid lifecycle works |

Protocol crate has round-trip and unknown-command rejection tests. The web build (`pnpm build`) runs `tsc -b`, so type drift between the TS mirror and hand-written client code fails the build.

## Manual demo verification (performed 2026-07-14, Chrome on macOS)

1. Console loads from the runtime binary, paints idle world, START begins the run.
2. Coverage policy fires when a patrol leaves zone Bravo; approving creates an assignment, the asset returns, the zone turns covered again.
3. Flag approval cascades into an investigate recommendation; approving tasks the observer; when the target's uplink dies mid-investigation the assignment completes with outcome "track lost".
4. Recommendation TTL expiry appears as `expired (runtime)` in the timeline.
5. Operator fault injection: setting l-north to unavailable stales then loses all tracks on that link; uncertainty halos grow on screen; comms policy flags the outage.
6. Browser closed while running: SQLite snapshot times keep advancing with zero clients (verified by polling the db). Reopening repaints current state and backfills the timeline from `recent_events`.
7. Scenario completes at exactly T+05:00 with a run metrics card.
8. Replay mode streams the recorded run: identical recommendation ids and operator actions, transport bar with pause/restart and 0.5x to 8x speeds, approve/decline controls absent.
9. Public bind without a token refuses to start.

## Not yet covered

- Automated tests for the runtime server layer (WebSocket session, store) beyond compilation; the sim/replay sessions are exercised manually.
- Load behavior with many clients or very large entity counts.
- Cross-compilation targets (edge deployment is gated on an external need; `cargo build --target aarch64-unknown-linux-gnu` is the intended path).
