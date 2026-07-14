# ADR 0001: Determinism rules

Status: accepted, 2026-07-14

## Context

Reproducibility is the product property everything else leans on: replay
must be trustworthy, policy configurations must be comparable on numbers,
and bug reports must be replayable from a scenario and a seed.

## Decision

Three rules, enforced by convention and tests:

1. Fixed 100 ms timestep; wall time never enters domain logic. Sim time is
   the only clock in `dira-domain` and `dira-simulator`.
2. One seeded ChaCha8 stream with a fixed draw order. Every spawned track
   consumes exactly one draw per tick on a lossy or dead link, even when the
   draw is unused, so a link recovering cannot shift the stream consumed by
   other tracks. `rand`/`rand_chacha` versions are pinned; ChaCha8 output is
   algorithmically stable across platforms.
3. Ordered iteration everywhere results can accumulate: entities in Vecs in
   scenario order, metric accumulators on BTreeMaps because float summation
   order matters for bit-exact equality, and no randomized-order HashMap
   iteration in any code path that produces events or metrics.

## Consequences

Two runs from the same scenario and seed produce byte-identical event
streams and metrics (enforced by `determinism.rs` and property tests).
Client connect/disconnect deliberately stays out of the domain event stream
(tracing only), because it would make the audit log depend on who was
watching. The cost is discipline: any new engine code must obey the three
rules, and the property tests exist to catch violations.
