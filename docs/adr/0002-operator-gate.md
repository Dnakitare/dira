# ADR 0002: The operator gate is an invariant, not a UI convention

Status: accepted, 2026-07-14

## Context

Decision-support systems drift toward auto-execution one convenience at a
time. The credibility of this runtime rests on the opposite guarantee: the
system recommends, a human decides, and the audit log proves which was which.

## Decision

Recommendations and assignments are different types with different
lifecycles. Policies can only produce `Recommendation`s (reason, policy id,
confidence, TTL). The only path to an `Assignment` is an explicit approve
command, which is validated against the *current* world (the asset may have
been tasked meanwhile, the track may be gone) and fails closed. Approval,
decline, acknowledgment, and expiry are all distinct audit events carrying
who acted (`operator`, `scripted-operator`, `runtime`).

The scripted operator used by benchmark mode goes through the exact same
`apply()` path as a human. There is no privileged internal shortcut.

## Consequences

A property test generates random scenarios and asserts that with no
approvals, zero assignments ever exist. The demo can show a recommendation
expiring unhandled — the runtime cleans up rather than acting. The cost is
that fully autonomous behavior cannot be added without changing the type
system, which is the point.
