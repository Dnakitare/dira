# ADR 0003: SQLite as the event log

Status: accepted, 2026-07-14

## Context

The runtime needs durable, portable storage for audit events, replay data,
and run metadata, on hardware ranging from a laptop to a small ARM box,
with no external services.

## Decision

One SQLite file (WAL mode), three tables: `runs`, `events` (append-only,
`(run_id, seq)` primary key), and `snapshots` (full world every 200 ms of
sim time). Writes happen synchronously in the sim task: one writer, local
disk, sub-millisecond statements at a 10 Hz tick — measured, not assumed.

Considered and rejected for the current scale:
- **Message broker / Kafka-shaped log**: an external service to install,
  monitor, and secure on an edge box, for a single-writer single-node log.
- **Flat JSONL files**: append is easy but replay seek, run listing, and
  partial reads all become bespoke code that SQLite gives for free.
- **Postgres**: wrong deployment weight for "scp one binary to the device."

## Consequences

Replay is a `SELECT ... ORDER BY`, the runs list is a query, and the whole
audit trail is one copyable file. Snapshots at 200 ms granularity cost about
17 MB per 5-minute run at demo scale, which is acceptable; the `every_ms`
thinning in `dira export` exists for sharing. If multi-node ever becomes
real, this decision gets revisited (see ADR 0005).
