//! Shared domain model for the dira runtime: world state, audit events,
//! policy evaluation, and run metrics.
//!
//! This crate is deliberately free of I/O, async, and randomness. Everything
//! here must behave identically in simulation, replay, benchmark, and edge
//! modes.

pub mod events;
pub mod metrics;
pub mod policy;
pub mod types;
pub mod world;

pub use events::{DomainEvent, Event};
pub use metrics::{MetricsAccumulator, RunMetrics};
pub use policy::{evaluate_policies, PolicyConfig, PolicyMemory};
pub use types::*;
pub use world::WorldState;
