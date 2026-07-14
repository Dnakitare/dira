# Demo script (about 3 minutes)

Setup: `cargo run -p dira-runtime -- simulate --scenario scenarios/perimeter-incursion.toml`, browser at http://127.0.0.1:8080. Everything below is one continuous run of the flagship scenario, seed 42, so it plays out the same way every time.

**Beat 1 — the picture (0:00).** Press START. Two protected zones, three friendly assets patrolling, tracks acquired through two sensor uplinks. Point out the SIMULATION badge and that colors are symbology: cyan friendly, amber unknown, red flagged.

**Beat 2 — the operator loop (0:40).** Around T+00:40 zone Bravo loses coverage as its patrol walks away, and the coverage policy recommends re-tasking with a reason and confidence. Approve it: an assignment appears, the asset turns around, the zone goes green. Recommendation to approval to assignment to effect, all as audit events in the timeline. Let one recommendation expire to show the runtime cleans up after inattention.

**Beat 3 — degradation is visible (1:00–2:30).** The scenario degrades the north uplink on a timeline: delayed at T+01:00, intermittent at T+01:30, dead at T+02:30. Watch the uncertainty halos on north tracks grow as their data ages, statuses fall from active to stale to lost, and the comms policy flag the link. The picture degrades honestly instead of pretending.

**Beat 4 — the browser is not the system (2:30).** Close the tab mid-run. Reopen it. The clock jumped forward and the timeline backfilled: the runtime never stopped. This is the edge-hosting argument in one gesture.

**Beat 5 — replay and numbers (3:00).** Let the run complete at T+05:00 and show the metrics card (coverage continuity, time to flag, response latency, tracks lost). Then `dira replay --db dira.db --run 1 --speed 4`: the exact run streams back, including the operator's own approvals, at 4x.

Optional coda for a technical audience: `dira benchmark --runs 10` with and without `--policy scenarios/policy-conservative.toml`, diff the aggregate JSON, and note the determinism tests in CI.
