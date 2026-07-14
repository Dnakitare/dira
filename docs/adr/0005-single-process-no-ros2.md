# ADR 0005: Single process, no ROS 2, no microservices

Status: accepted, 2026-07-14

## Context

The robotics-adjacent default stack for this kind of system is ROS 2 nodes,
a DDS bus, and often a container orchestrator. The actual requirement is
one authoritative state machine, one UI, and one auditable log, deployable
as a file.

## Decision

One Rust process: engine, policies, WebSocket server, and event log in a
single binary with modes selected by CLI. Concurrency is tokio tasks and
channels, not processes and a bus. Hardware abstraction is a narrow typed
seam (the engine consumes observations; the simulator and the replay reader
are the two current producers), not a middleware layer.

ROS 2 was rejected because nothing here needs its ecosystem yet: no drivers,
no perception stack, no multi-vendor node graph. Adopting it would trade a
`scp`-able static binary for a dependency stack heavier than the entire
project, to solve problems this project does not have. If a partner
integration ever requires ROS 2, the right shape is an adapter at the
observation seam, not a rewrite.

## Consequences

Deployment is: copy binary, copy scenario file, run. The whole system is
debuggable with a single log stream and reproducible with a seed. The cost
is that horizontal scale-out has no story yet — deliberately, per the
non-goals — and the observation seam is the contract that keeps that future
option open.
