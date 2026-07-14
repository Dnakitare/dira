//! SQLite persistence: an append-oriented audit log of runs, events, and
//! periodic world snapshots, used for the runs list, replay, and post-run
//! reporting. WAL mode; writes happen inline in the sim task (single writer,
//! local disk, sub-millisecond statements).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use dira_domain::{Event, RunMetrics, WorldState};
use dira_protocol::RunInfo;

pub struct Store {
    conn: Connection,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("opening event log {}", path.display()))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS runs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scenario_id TEXT NOT NULL,
                scenario_name TEXT NOT NULL,
                seed INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                completed INTEGER NOT NULL DEFAULT 0,
                duration_ms INTEGER,
                metrics_json TEXT
            );
            CREATE TABLE IF NOT EXISTS events (
                run_id INTEGER NOT NULL,
                seq INTEGER NOT NULL,
                sim_time_ms INTEGER NOT NULL,
                wall_time_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL,
                PRIMARY KEY (run_id, seq)
            );
            CREATE TABLE IF NOT EXISTS snapshots (
                run_id INTEGER NOT NULL,
                sim_time_ms INTEGER NOT NULL,
                payload_json TEXT NOT NULL,
                PRIMARY KEY (run_id, sim_time_ms)
            );",
        )?;
        Ok(Self { conn })
    }

    pub fn begin_run(&self, world: &WorldState) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO runs (scenario_id, scenario_name, seed, started_at_ms) VALUES (?1, ?2, ?3, ?4)",
            params![world.scenario_id, world.scenario_name, world.seed as i64, now_ms() as i64],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn record_event(&self, run_id: i64, event: &Event) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO events (run_id, seq, sim_time_ms, wall_time_ms, payload_json)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                run_id,
                event.seq as i64,
                event.sim_time_ms as i64,
                now_ms() as i64,
                serde_json::to_string(event)?,
            ],
        )?;
        Ok(())
    }

    pub fn record_snapshot(&self, run_id: i64, world: &WorldState) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO snapshots (run_id, sim_time_ms, payload_json) VALUES (?1, ?2, ?3)",
            params![run_id, world.sim_time_ms as i64, serde_json::to_string(world)?],
        )?;
        Ok(())
    }

    pub fn finish_run(
        &self,
        run_id: i64,
        completed: bool,
        duration_ms: u64,
        metrics: Option<&RunMetrics>,
    ) -> Result<()> {
        let metrics_json = metrics.map(serde_json::to_string).transpose()?;
        self.conn.execute(
            "UPDATE runs SET completed = ?2, duration_ms = ?3, metrics_json = ?4 WHERE id = ?1",
            params![run_id, completed as i64, duration_ms as i64, metrics_json],
        )?;
        Ok(())
    }

    pub fn list_runs(&self) -> Result<Vec<RunInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, scenario_id, seed, started_at_ms, completed FROM runs ORDER BY id DESC LIMIT 50",
        )?;
        let rows = stmt
            .query_map([], |row| {
                Ok(RunInfo {
                    run_id: row.get(0)?,
                    scenario_id: row.get(1)?,
                    seed: row.get::<_, i64>(2)? as u64,
                    started_at_ms: row.get::<_, i64>(3)? as u64,
                    completed: row.get::<_, i64>(4)? != 0,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn load_run(&self, run_id: i64) -> Result<RecordedRun> {
        let info = self
            .conn
            .query_row(
                "SELECT id, scenario_id, seed, started_at_ms, completed FROM runs WHERE id = ?1",
                params![run_id],
                |row| {
                    Ok(RunInfo {
                        run_id: row.get(0)?,
                        scenario_id: row.get(1)?,
                        seed: row.get::<_, i64>(2)? as u64,
                        started_at_ms: row.get::<_, i64>(3)? as u64,
                        completed: row.get::<_, i64>(4)? != 0,
                    })
                },
            )
            .optional()?
            .with_context(|| format!("run {run_id} not found in event log"))?;

        let mut stmt = self
            .conn
            .prepare("SELECT payload_json FROM events WHERE run_id = ?1 ORDER BY seq")?;
        let events = stmt
            .query_map(params![run_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<String>>>()?
            .iter()
            .map(|s| serde_json::from_str::<Event>(s))
            .collect::<serde_json::Result<Vec<_>>>()?;

        let mut stmt = self.conn.prepare(
            "SELECT sim_time_ms, payload_json FROM snapshots WHERE run_id = ?1 ORDER BY sim_time_ms",
        )?;
        let snapshots = stmt
            .query_map(params![run_id], |row| {
                Ok((row.get::<_, i64>(0)? as u64, row.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
            .iter()
            .map(|(t, s)| serde_json::from_str::<WorldState>(s).map(|w| (*t, w)))
            .collect::<serde_json::Result<Vec<_>>>()?;

        Ok(RecordedRun {
            info,
            events,
            snapshots,
        })
    }
}

pub struct RecordedRun {
    pub info: RunInfo,
    pub events: Vec<Event>,
    pub snapshots: Vec<(u64, WorldState)>,
}
