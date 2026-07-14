//! HTTP + WebSocket surface shared by simulate, edge, and replay modes.
//!
//! The server never owns state; it forwards operator commands to whichever
//! task owns the engine (or replay stream) and fans broadcast messages out to
//! clients. The browser stays a display/control client only.

use std::collections::{HashMap, VecDeque};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use anyhow::{bail, Context, Result};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{broadcast, mpsc};
use tower_http::services::{ServeDir, ServeFile};

use dira_domain::{Event, WorldState};
use dira_protocol::{
    decode_client, ClientEnvelope, RunInfo, RuntimeInfo, ScenarioInfo, ServerEnvelope, ServerMsg,
    PROTOCOL_VERSION,
};

pub const RECENT_EVENTS_CAP: usize = 300;

/// A command from one client plus the channel for its direct replies
/// (acks and errors go only to the sender; events and snapshots broadcast).
pub struct ClientCommand {
    pub env: ClientEnvelope,
    pub reply: mpsc::Sender<String>,
}

/// Data every new connection needs to paint immediately.
pub struct HelloData {
    pub runtime: RuntimeInfo,
    pub scenarios: Vec<ScenarioInfo>,
    pub runs: Vec<RunInfo>,
    pub world: WorldState,
    pub recent: VecDeque<Event>,
}

pub struct AppState {
    pub cmd_tx: mpsc::Sender<ClientCommand>,
    pub broadcast_tx: broadcast::Sender<String>,
    pub hello: RwLock<HelloData>,
    pub seq: AtomicU64,
    pub token: Option<String>,
}

impl AppState {
    pub fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    pub fn encode(&self, msg: ServerMsg) -> String {
        dira_protocol::encode_server(&ServerEnvelope {
            v: PROTOCOL_VERSION,
            seq: self.next_seq(),
            msg,
        })
    }

    /// Broadcast to all clients and mirror into the hello cache so new
    /// connections see a consistent picture.
    pub fn publish_event(&self, event: &Event) {
        {
            let mut hello = self.hello.write().expect("hello lock");
            hello.recent.push_back(event.clone());
            while hello.recent.len() > RECENT_EVENTS_CAP {
                hello.recent.pop_front();
            }
        }
        let _ = self
            .broadcast_tx
            .send(self.encode(ServerMsg::Event { event: event.clone() }));
    }

    pub fn publish_snapshot(&self, world: &WorldState) {
        self.hello.write().expect("hello lock").world = world.clone();
        let _ = self
            .broadcast_tx
            .send(self.encode(ServerMsg::Snapshot { world: world.clone() }));
    }

    pub fn publish_raw(&self, msg: ServerMsg) {
        let _ = self.broadcast_tx.send(self.encode(msg));
    }
}

pub fn require_token_for_public_bind(bind: &SocketAddr, token: &Option<String>) -> Result<()> {
    if !bind.ip().is_loopback() && token.is_none() {
        bail!(
            "refusing to bind {} without an auth token; pass --token (or set one in the edge config)",
            bind
        );
    }
    Ok(())
}

pub async fn serve(state: Arc<AppState>, bind: SocketAddr, web_dir: Option<String>) -> Result<()> {
    let mut app = Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/health", get(health))
        .with_state(state);

    if let Some(dir) = web_dir {
        let index = format!("{dir}/index.html");
        if std::path::Path::new(&index).exists() {
            app = app.fallback_service(ServeDir::new(&dir).fallback(ServeFile::new(index)));
        } else {
            tracing::warn!("web dir {dir} has no index.html; UI will not be served");
        }
    }

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("binding {bind}"))?;
    tracing::info!("listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<Arc<AppState>>) -> Response {
    let (mode, scenario, phase) = {
        let hello = state.hello.read().expect("hello lock");
        (
            hello.runtime.mode.clone(),
            hello.world.scenario_id.clone(),
            hello.world.phase,
        )
    };
    axum::Json(serde_json::json!({
        "status": "ok",
        "mode": mode,
        "scenario": scenario,
        "phase": phase,
        "protocol_version": PROTOCOL_VERSION,
    }))
    .into_response()
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if let Some(expected) = &state.token {
        if params.get("token") != Some(expected) {
            return (axum::http::StatusCode::UNAUTHORIZED, "missing or bad token").into_response();
        }
    }
    ws.on_upgrade(move |socket| client_loop(socket, state))
}

async fn client_loop(socket: WebSocket, state: Arc<AppState>) {
    let (mut sink, mut stream) = socket.split();
    // Subscribe before building the hello so no broadcast is missed between
    // snapshot and stream start.
    let mut bcast = state.broadcast_tx.subscribe();
    let (reply_tx, mut reply_rx) = mpsc::channel::<String>(64);

    let hello = {
        let h = state.hello.read().expect("hello lock");
        state.encode(ServerMsg::Hello {
            runtime: h.runtime.clone(),
            scenarios: h.scenarios.clone(),
            runs: h.runs.clone(),
            world: h.world.clone(),
            recent_events: h.recent.iter().cloned().collect(),
        })
    };
    if sink.send(Message::Text(hello.into())).await.is_err() {
        return;
    }
    tracing::info!("client connected");

    loop {
        tokio::select! {
            res = bcast.recv() => match res {
                Ok(msg) => {
                    if sink.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("client lagged {n} messages; continuing");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            },
            Some(msg) = reply_rx.recv() => {
                if sink.send(Message::Text(msg.into())).await.is_err() {
                    break;
                }
            }
            incoming = stream.next() => match incoming {
                Some(Ok(Message::Text(text))) => {
                    handle_incoming(&state, &reply_tx, text.as_str()).await;
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {}
                Some(Err(_)) => break,
            },
        }
    }
    tracing::info!("client disconnected");
}

async fn handle_incoming(state: &Arc<AppState>, reply_tx: &mpsc::Sender<String>, text: &str) {
    let env = match decode_client(text) {
        Ok(env) => env,
        Err(err) => {
            let msg = state.encode(ServerMsg::Error {
                command_id: None,
                code: "bad_message".into(),
                message: format!("could not parse command: {err}"),
            });
            let _ = reply_tx.send(msg).await;
            return;
        }
    };
    if env.v != PROTOCOL_VERSION {
        let msg = state.encode(ServerMsg::Error {
            command_id: Some(env.id),
            code: "bad_version".into(),
            message: format!(
                "protocol version {} not supported (runtime speaks {})",
                env.v, PROTOCOL_VERSION
            ),
        });
        let _ = reply_tx.send(msg).await;
        return;
    }
    if state
        .cmd_tx
        .send(ClientCommand {
            env,
            reply: reply_tx.clone(),
        })
        .await
        .is_err()
    {
        let msg = state.encode(ServerMsg::Error {
            command_id: None,
            code: "runtime_unavailable".into(),
            message: "runtime task is not accepting commands".into(),
        });
        let _ = reply_tx.send(msg).await;
    }
}
