//! Versioned WebSocket wire protocol between the runtime and browser clients.
//!
//! Every message carries `v` (protocol version). Server messages also carry a
//! per-connection-stream `seq` so a client can detect gaps. The browser client
//! maintains hand-written TypeScript mirrors of these types; `docs/protocol.md`
//! is the contract.

use serde::{Deserialize, Serialize};

use dira_domain::{Event, FaultSpec, WorldState};

pub const PROTOCOL_VERSION: u32 = 1;

/// Server -> client messages.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ServerMsg {
    /// First message on every connection: everything a client needs to paint.
    Hello {
        runtime: RuntimeInfo,
        scenarios: Vec<ScenarioInfo>,
        runs: Vec<RunInfo>,
        world: WorldState,
        /// Recent audit events so a reconnecting client can backfill its timeline.
        recent_events: Vec<Event>,
    },
    /// Full world snapshot. Sent every sim tick while running.
    Snapshot { world: WorldState },
    /// One audit event, sent as it occurs.
    Event { event: Event },
    /// A command was accepted and applied.
    CommandAck { command_id: u64 },
    /// A command was rejected. Commands fail closed.
    Error {
        command_id: Option<u64>,
        code: String,
        message: String,
    },
    /// Replay-mode progress.
    ReplayStatus {
        run_id: i64,
        speed: f64,
        position_ms: u64,
        duration_ms: u64,
        playing: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServerEnvelope {
    pub v: u32,
    pub seq: u64,
    #[serde(flatten)]
    pub msg: ServerMsg,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInfo {
    pub name: String,
    pub version: String,
    /// "simulate" | "replay" | "edge"
    pub mode: String,
    pub protocol_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenarioInfo {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunInfo {
    pub run_id: i64,
    pub scenario_id: String,
    pub seed: u64,
    /// Wall-clock start, unix epoch milliseconds. Clients format for display.
    pub started_at_ms: u64,
    pub completed: bool,
}

/// Client -> server command envelope. `id` is client-chosen and echoed back
/// in the CommandAck/Error so the UI can correlate. The command is nested
/// (not flattened) so envelope fields can never collide with command fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClientEnvelope {
    pub v: u32,
    pub id: u64,
    pub command: Command,
}

/// The complete set of operator actions. Anything not listed here cannot be
/// done from a client.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Start,
    Pause,
    Resume,
    Reset,
    SelectScenario { scenario_id: String },
    AckRecommendation { id: String },
    ApproveRecommendation { id: String },
    DeclineRecommendation { id: String },
    InjectFault { fault: FaultSpec },
    /// Replay mode only.
    SetSpeed { multiplier: f64 },
}

pub fn encode_server(env: &ServerEnvelope) -> String {
    serde_json::to_string(env).expect("server message serialization is infallible")
}

pub fn decode_client(text: &str) -> Result<ClientEnvelope, serde_json::Error> {
    serde_json::from_str(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_envelope_round_trips() {
        let env = ClientEnvelope {
            v: PROTOCOL_VERSION,
            id: 7,
            command: Command::ApproveRecommendation {
                id: "R-0001".to_string(),
            },
        };
        let text = serde_json::to_string(&env).unwrap();
        assert_eq!(
            text,
            r#"{"v":1,"id":7,"command":{"type":"approve_recommendation","id":"R-0001"}}"#
        );
        assert_eq!(decode_client(&text).unwrap(), env);
    }

    #[test]
    fn unknown_command_fails_closed() {
        let res = decode_client(r#"{"v":1,"id":1,"command":{"type":"launch_everything"}}"#);
        assert!(res.is_err());
    }
}
