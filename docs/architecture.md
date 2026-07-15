# Architecture

## Authority model

One process owns the world. `dira-simulator::SimEngine` holds the single authoritative `WorldState` and is the only code that mutates it. Everything else is a view or a command source:

- The **runtime** (axum server) broadcasts snapshots and audit events to WebSocket clients and forwards their commands to the engine.
- The **browser console** renders snapshots and sends commands. It holds no truth. If it disconnects, the engine does not notice or care; on reconnect the hello message carries the full world plus recent events and the client repaints from scratch.
- The **event log** (SQLite) is an append-oriented record of everything that happened: events, periodic snapshots, run metadata, final metrics.

Commands fail closed. Unknown ids, wrong phases, malformed JSON, and protocol version mismatches are rejected with an error message to the sending client only; they never partially apply.

## Crate map

```
domain     types, WorldState, policy evaluation, metrics accumulation
           no I/O, no clocks, no randomness: everything here must behave
           identically in simulate, replay, benchmark, and edge modes
protocol   ServerMsg / Command envelopes, versioned, serde JSON
simulator  scenario TOML loading/validation + the deterministic engine
runtime    the binary: server, sqlite store, sim/replay sessions, benchmark
```

Dependency direction is strictly downward: runtime -> {simulator, protocol, domain}; simulator -> domain; protocol -> domain.

## Determinism

Three rules make runs reproducible:

1. **Fixed timestep.** The engine advances in 100 ms sim-time ticks. Wall time never enters domain logic; it exists only in the audit log's `wall_time_ms` column.
2. **One seeded RNG stream.** ChaCha8, seeded from the scenario. Draw order is fixed: each spawned track consumes exactly one draw per tick on a lossy or dead link (dead links still consume a draw so recovery does not shift other tracks' streams).
3. **Ordered iteration.** Entities live in Vecs in scenario order. Metric accumulators use BTreeMaps, since float summation order matters for bit-exact equality.

`tests/determinism.rs` runs the flagship scenario twice and asserts the two event streams and final metrics are identical, and that a different seed diverges.

## Simulation of degraded links

The engine keeps ground truth separate from the operating picture. Truth tracks move; links decide what the picture sees:

- **nominal**: observation delivered same tick.
- **delayed**: observation queued, delivered later, stamped with its measurement time. Late data never regresses fresher data.
- **intermittent**: seeded coin flip per observation.
- **unavailable**: nothing delivered.

Track uncertainty is `20 m + 15 m/s x observation age`, capped at 500 m. Age also drives status: active (< 3 s), stale (< 20 s), lost (< 45 s), then dropped from the picture. The console renders uncertainty as a halo, so link failures are visible as geometry, not just as a status chip.

## Policies

Four policies evaluate every tick against the current picture (crates/domain/src/policy.rs): zone coverage, track flagging (inside a protected zone, or projected entry within a horizon), investigation of flagged tracks, and comms checks on persistently degraded links. Each recommendation carries policy id, reason, confidence, and a TTL. Dedupe keys prevent nagging; operator decisions are remembered per subject per episode.

A recommendation does nothing until approved. Approval validates against the current world (the asset may have been tasked meanwhile, the track may be gone) and either creates an assignment or fails closed.

## Storage

```
runs(id, scenario_id, scenario_name, seed, started_at_ms, completed, duration_ms, metrics_json)
events(run_id, seq, sim_time_ms, wall_time_ms, payload_json)
snapshots(run_id, sim_time_ms, payload_json)      -- every 200 ms of sim time
```

WAL mode, single writer (the sim session). Replay mode reads a run and streams events and snapshots back on the same wire protocol; the console works identically except command surface shrinks to transport controls.

## Modes

| mode      | engine        | inputs                   | typical use                     |
|-----------|---------------|--------------------------|---------------------------------|
| simulate  | live          | scenario file            | development, demos              |
| edge      | live          | scenario file (for now)  | long-lived service on a device  |
| replay    | none          | recorded run             | audit, debrief                  |
| benchmark | live, headless| scenario file, seed range| policy comparison               |

Edge mode is simulate mode with a config file and service posture; the seam for real input adapters is the same typed-observation path the simulator uses. That is deliberate: replacing synthetic sources must not touch domain logic.

## Presentation layer

Two things exist purely for the operator's eyes and are never read by simulation or policy code:

- **Basemap.** A scenario may carry an `origin` (lat/lon) and a pre-rendered map texture with a known extent. The runtime passes it through in `WorldState.basemap`; the console drapes it as a plane under the scene. Textures are generated offline from the Protomaps daily build (see `web/public/basemaps/`), so there is no runtime tile dependency and it works air-gapped. Attribution is shown whenever a basemap is present.
- **Symbology.** Track shape encodes domain (arc = air, square = ground, diamond = unknown), following MIL-STD-2525 convention; color encodes identity (amber unknown, red flagged, cyan friendly). The distinction lives entirely in the renderer; the wire model just carries `class`.

Both are additive: a scenario with no `origin` renders on the abstract grid, and the shape logic has a defined glyph for every class.

## Known limitations

- Snapshots are full-state, not deltas. Fine at this entity count; the protocol envelope leaves room for a delta message kind later.
- Client connect/disconnect is logged to tracing, not to the audit event stream, to keep the domain event stream a pure function of scenario + seed + operator commands.
- Single node, single process. Multi-node state sync is out of scope until something real needs it.
