# Wire protocol (v1)

JSON over WebSocket at `/ws`. Every message carries `v` (protocol version). The runtime rejects commands whose `v` it does not speak; clients should treat an unknown server `kind` as ignorable. Rust types in `crates/protocol` are the source of truth; `web/src/protocol/types.ts` mirrors them by hand and must be updated in the same change.

Auth: if the runtime was started with a token, clients pass `?token=...` on the WebSocket URL. Non-loopback binds require a token.

## Server to client

Envelope: `{ "v": 1, "seq": N, "kind": "...", ...payload }`. `seq` is a server-global monotonic counter.

| kind | payload | when |
|------|---------|------|
| `hello` | `runtime`, `scenarios`, `runs`, `world`, `recent_events` | first message on every connection; full repaint |
| `snapshot` | `world` | every tick while running; 1 Hz heartbeat otherwise |
| `event` | `event` (audit event) | as events occur |
| `command_ack` | `command_id` | command accepted and applied |
| `error` | `command_id?`, `code`, `message` | command rejected (fail closed) |
| `replay_status` | `run_id`, `speed`, `position_ms`, `duration_ms`, `playing` | replay mode, ~1 Hz and after transport commands |

`world` is the full `WorldState`: scenario metadata, phase, sim clock, zones, assets, tracks, links, nodes, recommendations, assignments. Audit events are `{ seq, sim_time_ms, type, ...fields }` with `type` in snake_case (`track_appeared`, `link_state_changed`, `recommendation_issued`, ...). Event `seq` is per-run and independent of the envelope `seq`.

## Client to server

`{ "v": 1, "id": N, "command": { "type": "...", ... } }`. `id` is client-chosen and echoed in `command_ack`/`error`.

Simulate/edge commands: `start`, `pause`, `resume`, `reset`, `select_scenario {scenario_id}`, `ack_recommendation {id}`, `approve_recommendation {id}`, `decline_recommendation {id}`, `inject_fault {fault}`.

Replay commands: `pause`, `resume`, `reset` (restart), `set_speed {multiplier}`.

Anything else, or a valid command in the wrong mode or phase, gets an `error` with code `rejected`. This list is the complete operator surface; there is no side channel.

`fault` is either `{ "target": "link", "link": id, "state": {...} }` or `{ "target": "node", "node": id, "health": "nominal|degraded|unavailable" }`, where link state is `{"state":"nominal"}`, `{"state":"delayed","delay_ms":N}`, `{"state":"intermittent","loss":0..1}`, or `{"state":"unavailable"}`.

## Versioning rules

- Additive changes (new message kinds, new optional fields): same version. Clients ignore unknown kinds and fields.
- Breaking changes (field renames/removals, semantic changes): bump `PROTOCOL_VERSION` in `crates/protocol` and the TS mirror in the same commit. The runtime rejects mismatched command envelopes with code `bad_version`, which the console surfaces to the operator.
