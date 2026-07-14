//! Deterministic fixed-timestep scenario engine.
//!
//! The engine owns the authoritative `WorldState` and is the only thing that
//! mutates it. All randomness comes from one seeded ChaCha8 stream, all time
//! is simulation time, and all iteration is insertion-ordered, so the same
//! scenario file and seed always produce the same event stream.

pub mod engine;
pub mod scenario;

pub use engine::{run_scripted, OperatorAction, SimEngine};
pub use scenario::{load_scenario, Scenario, ScenarioError};
