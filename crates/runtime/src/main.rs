//! dira: a compiled, edge-hosted digital-twin and operator-coordination
//! runtime. One binary, four modes, one shared engine.

mod benchmark;
mod replay;
mod server;
mod sim;
mod store;

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, RwLock};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use tokio::sync::{broadcast, mpsc};

use dira_protocol::{RuntimeInfo, PROTOCOL_VERSION};
use dira_simulator::load_scenario;

use server::{AppState, HelloData};
use sim::{apply_policy_override, SimSession};
use store::Store;

#[derive(Parser)]
#[command(
    name = "dira",
    version,
    about = "Edge digital-twin runtime: deterministic simulation, operator decision support, auditable replay"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Run a synthetic scenario and serve the operator UI.
    Simulate {
        #[arg(long)]
        scenario: PathBuf,
        /// Policy configuration override (TOML).
        #[arg(long)]
        policy: Option<PathBuf>,
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: SocketAddr,
        #[arg(long, default_value = "dira.db")]
        db: PathBuf,
        #[arg(long, default_value = "web/dist")]
        web_dir: String,
        /// Required when binding a non-loopback address.
        #[arg(long)]
        token: Option<String>,
    },
    /// Run as a long-lived edge service configured from a file.
    Edge {
        #[arg(long)]
        config: PathBuf,
    },
    /// Stream a recorded run back to the UI.
    Replay {
        #[arg(long, default_value = "dira.db")]
        db: PathBuf,
        #[arg(long)]
        run: i64,
        #[arg(long, default_value = "127.0.0.1:8080")]
        bind: SocketAddr,
        #[arg(long, default_value = "web/dist")]
        web_dir: String,
        #[arg(long, default_value_t = 1.0)]
        speed: f64,
        #[arg(long)]
        token: Option<String>,
    },
    /// Headless repeated runs with a scripted operator; prints metrics JSON.
    Benchmark {
        #[arg(long, required_unless_present = "stress")]
        scenario: Option<PathBuf>,
        #[arg(long)]
        policy: Option<PathBuf>,
        #[arg(long, default_value_t = 5)]
        runs: u32,
        #[arg(long, default_value_t = 2000)]
        approve_after_ms: u64,
        /// Defaults to the scenario's own seed; run i uses base_seed + i.
        #[arg(long)]
        base_seed: Option<u64>,
        /// Synthesize an N-track scenario and report tick-time percentiles
        /// instead of running metric comparisons.
        #[arg(long)]
        stress: Option<usize>,
    },
    /// Export a recorded run as a single JSON file (for sharing or the
    /// static demo player).
    Export {
        #[arg(long, default_value = "dira.db")]
        db: PathBuf,
        #[arg(long)]
        run: i64,
        #[arg(long)]
        out: PathBuf,
        /// Keep at most one snapshot per this many sim milliseconds.
        #[arg(long, default_value_t = 200)]
        every_ms: u64,
    },
}

/// Edge mode configuration file.
#[derive(Debug, Deserialize)]
struct EdgeConfig {
    scenario: PathBuf,
    policy: Option<PathBuf>,
    #[serde(default = "default_bind")]
    bind: SocketAddr,
    #[serde(default = "default_db")]
    db: PathBuf,
    #[serde(default = "default_web_dir")]
    web_dir: String,
    token: Option<String>,
}

fn default_bind() -> SocketAddr {
    "127.0.0.1:8080".parse().expect("valid default bind")
}

fn default_db() -> PathBuf {
    PathBuf::from("dira.db")
}

fn default_web_dir() -> String {
    "web/dist".to_string()
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Simulate {
            scenario,
            policy,
            bind,
            db,
            web_dir,
            token,
        } => serve_sim(
            "simulate",
            &scenario,
            policy.as_deref(),
            bind,
            &db,
            web_dir,
            token,
        ),
        Cmd::Edge { config } => {
            let text = std::fs::read_to_string(&config)
                .with_context(|| format!("reading edge config {}", config.display()))?;
            let cfg: EdgeConfig = toml::from_str(&text)
                .with_context(|| format!("parsing edge config {}", config.display()))?;
            serve_sim(
                "edge",
                &cfg.scenario,
                cfg.policy.as_deref(),
                cfg.bind,
                &cfg.db,
                cfg.web_dir,
                cfg.token,
            )
        }
        Cmd::Replay {
            db,
            run,
            bind,
            web_dir,
            speed,
            token,
        } => serve_replay(&db, run, bind, web_dir, speed, token),
        Cmd::Benchmark {
            scenario,
            policy,
            runs,
            approve_after_ms,
            base_seed,
            stress,
        } => match stress {
            Some(n) => benchmark::stress(n),
            None => benchmark::run(
                scenario
                    .as_deref()
                    .expect("clap enforces scenario or stress"),
                policy.as_deref(),
                runs,
                approve_after_ms,
                base_seed,
            ),
        },
        Cmd::Export {
            db,
            run,
            out,
            every_ms,
        } => export_run(&db, run, &out, every_ms),
    }
}

/// Write one recorded run as a self-contained JSON document:
/// `{ info, events, snapshots: [{sim_time_ms, world}] }`.
fn export_run(db: &Path, run_id: i64, out: &Path, every_ms: u64) -> Result<()> {
    let store = Store::open(db)?;
    let recorded = store.load_run(run_id)?;
    let mut snapshots = Vec::new();
    let mut last: Option<u64> = None;
    let n_total = recorded.snapshots.len();
    for (i, (t, world)) in recorded.snapshots.iter().enumerate() {
        let keep = last.is_none_or(|lt| t.saturating_sub(lt) >= every_ms) || i == n_total - 1;
        if keep {
            last = Some(*t);
            snapshots.push(serde_json::json!({ "sim_time_ms": t, "world": world }));
        }
    }
    let doc = serde_json::json!({
        "format": "dira-run-export/1",
        "info": recorded.info,
        "events": recorded.events,
        "snapshots": snapshots,
    });
    std::fs::write(out, serde_json::to_string(&doc)?)?;
    eprintln!(
        "exported run {run_id}: {} events, {}/{} snapshots -> {}",
        recorded.events.len(),
        snapshots.len(),
        n_total,
        out.display()
    );
    Ok(())
}

fn runtime_info(mode: &str) -> RuntimeInfo {
    RuntimeInfo {
        name: "dira".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        mode: mode.into(),
        protocol_version: PROTOCOL_VERSION,
    }
}

fn build_state(
    runtime: RuntimeInfo,
    token: Option<String>,
    world: dira_domain::WorldState,
) -> (Arc<AppState>, mpsc::Receiver<server::ClientCommand>) {
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let (broadcast_tx, _) = broadcast::channel(1024);
    let state = Arc::new(AppState {
        cmd_tx,
        broadcast_tx,
        hello: RwLock::new(HelloData {
            runtime,
            scenarios: Vec::new(),
            runs: Vec::new(),
            world,
            recent: VecDeque::new(),
        }),
        seq: AtomicU64::new(0),
        token,
    });
    (state, cmd_rx)
}

fn serve_sim(
    mode: &str,
    scenario_path: &Path,
    policy_path: Option<&Path>,
    bind: SocketAddr,
    db: &Path,
    web_dir: String,
    token: Option<String>,
) -> Result<()> {
    server::require_token_for_public_bind(&bind, &token)?;
    let policy_override = policy_path.map(sim::load_policy_file).transpose()?;
    let mut scenario = load_scenario(scenario_path)
        .with_context(|| format!("loading scenario {}", scenario_path.display()))?;
    apply_policy_override(&mut scenario, &policy_override);
    let store = Store::open(db)?;
    let runs = store.list_runs()?;

    tracing::info!(
        "mode={mode} scenario={} seed={} db={}",
        scenario.id,
        scenario.seed,
        db.display()
    );

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let world_preview = dira_simulator::SimEngine::new(scenario.clone())
            .world()
            .clone();
        let (state, cmd_rx) = build_state(runtime_info(mode), token, world_preview);
        let session = SimSession::new(
            state.clone(),
            store,
            scenario,
            scenario_path,
            policy_override,
        )?;
        {
            let mut hello = state.hello.write().expect("hello lock");
            hello.scenarios = session.scenario_infos();
            hello.runs = runs;
        }
        tokio::spawn(async move {
            if let Err(e) = session.run(cmd_rx).await {
                tracing::error!("sim session ended: {e:#}");
            }
        });
        server::serve(state, bind, Some(web_dir)).await
    })
}

fn serve_replay(
    db: &Path,
    run_id: i64,
    bind: SocketAddr,
    web_dir: String,
    speed: f64,
    token: Option<String>,
) -> Result<()> {
    server::require_token_for_public_bind(&bind, &token)?;
    let store = Store::open(db)?;
    let recorded = store.load_run(run_id)?;
    let runs = store.list_runs()?;
    tracing::info!(
        "mode=replay run={} scenario={} events={} snapshots={}",
        run_id,
        recorded.info.scenario_id,
        recorded.events.len(),
        recorded.snapshots.len()
    );

    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let first_world = recorded
            .snapshots
            .first()
            .map(|(_, w)| w.clone())
            .context("run has no snapshots to replay")?;
        let (state, cmd_rx) = build_state(runtime_info("replay"), token, first_world);
        state.hello.write().expect("hello lock").runs = runs;
        let session = replay::ReplaySession::new(state.clone(), recorded, speed)?;
        tokio::spawn(async move {
            if let Err(e) = session.run(cmd_rx).await {
                tracing::error!("replay session ended: {e:#}");
            }
        });
        server::serve(state, bind, Some(web_dir)).await
    })
}
