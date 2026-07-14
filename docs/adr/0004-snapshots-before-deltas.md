# ADR 0004: Full snapshots before deltas

Status: accepted, 2026-07-14

## Context

The wire protocol could ship full world snapshots every tick or maintain
per-client delta streams. Deltas are the "obviously right" answer that
carries real complexity: per-client sync state, gap repair, ordering rules
between deltas and events, and a whole class of desync bugs.

## Decision

Full snapshots at 10 Hz, plus discrete audit events. A reconnecting client
needs zero protocol machinery: the hello message is a snapshot plus recent
events, and every subsequent snapshot is self-sufficient. Client rendering
is a pure function of the latest snapshot.

## Consequences

Measured cost (docs/performance.md): 3.4 MB per snapshot at 10k tracks,
about 34 MB/s per client at 10 Hz — fine on localhost and LAN at demo
scale, clearly not fine for constrained links at large scale. That number
is the justification bar a delta implementation must clear, and the
envelope's `kind` field leaves room to add a `delta` message without
breaking version 1 clients. Decision revisits when a real deployment has a
real link budget.
