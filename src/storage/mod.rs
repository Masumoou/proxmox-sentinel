// src/storage/mod.rs
//
// Persistent storage via embedded SQLite.
// Stores metric snapshots, log lines, and alert history
// with configurable retention periods.

use anyhow::{Context, Result};
use rusqlite::{Connection, params};
use std::path::Path;
use std::sync::Mutex;
use tracing::info;

// ──────────────────────────────────────────────────────────────────────────────
// Storage config (added to main Config)
// ──────────────────────────────────────────────────────────────────────────────

/// Storage layer wrapping a SQLite connection.
/// Thread-safe via Mutex since SQLite is single-writer.
pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    /// Open (or create) the database at the given path.
    pub fn open(db_path: &Path) -> Result<Self> {
        // Ensure parent directory exists
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Creating storage dir: {}", parent.display()))?;
        }

        let conn = Connection::open(db_path)
            .with_context(|| format!("Opening database: {}", db_path.display()))?;

        // Performance tuning for a monitoring workload
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA cache_size = -8000;
             PRAGMA temp_store = MEMORY;
             PRAGMA busy_timeout = 5000;"
        )?;

        let storage = Self { conn: Mutex::new(conn) };
        storage.create_tables()?;
        info!("Storage opened: {}", db_path.display());
        Ok(storage)
    }

    // ── Schema ───────────────────────────────────────────────────────────────

    fn create_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            -- Node metrics snapshots
            CREATE TABLE IF NOT EXISTS node_metrics (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                ts        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                node      TEXT    NOT NULL,
                cpu       REAL    NOT NULL,
                mem_used  INTEGER NOT NULL,
                mem_total INTEGER NOT NULL,
                swap_used INTEGER NOT NULL DEFAULT 0,
                swap_total INTEGER NOT NULL DEFAULT 0,
                disk_used INTEGER NOT NULL,
                disk_total INTEGER NOT NULL,
                load_avg1 REAL    NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_node_metrics_ts ON node_metrics(ts);
            CREATE INDEX IF NOT EXISTS idx_node_metrics_node ON node_metrics(node);

            -- Guest metrics snapshots
            CREATE TABLE IF NOT EXISTS guest_metrics (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                ts        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                vmid      INTEGER NOT NULL,
                name      TEXT    NOT NULL,
                kind      TEXT    NOT NULL,
                status    TEXT    NOT NULL,
                cpu       REAL    NOT NULL,
                mem_used  INTEGER NOT NULL,
                mem_total INTEGER NOT NULL,
                node      TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_guest_metrics_ts ON guest_metrics(ts);
            CREATE INDEX IF NOT EXISTS idx_guest_metrics_vmid ON guest_metrics(vmid);

            -- Log lines
            CREATE TABLE IF NOT EXISTS log_lines (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                ts        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                source    TEXT    NOT NULL,
                severity  TEXT    NOT NULL DEFAULT 'info',
                line      TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_log_lines_ts ON log_lines(ts);
            CREATE INDEX IF NOT EXISTS idx_log_lines_source ON log_lines(source);

            -- Alert history
            CREATE TABLE IF NOT EXISTS alert_history (
                id        INTEGER PRIMARY KEY AUTOINCREMENT,
                ts        TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                alert_key TEXT    NOT NULL,
                severity  TEXT    NOT NULL,
                summary   TEXT    NOT NULL,
                resolved  INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_alert_history_ts ON alert_history(ts);
            CREATE INDEX IF NOT EXISTS idx_alert_history_key ON alert_history(alert_key);

            -- HAProxy snapshots
            CREATE TABLE IF NOT EXISTS haproxy_metrics (
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                ts           TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                proxy_name   TEXT    NOT NULL,
                server_name  TEXT    NOT NULL,
                status       TEXT    NOT NULL,
                sessions     INTEGER NOT NULL DEFAULT 0,
                bytes_in     INTEGER NOT NULL DEFAULT 0,
                bytes_out    INTEGER NOT NULL DEFAULT 0,
                http_5xx     INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_haproxy_ts ON haproxy_metrics(ts);
            "
        ).context("Creating database tables")?;
        Ok(())
    }

    // ── Insert functions ─────────────────────────────────────────────────────

    pub fn insert_node_metric(
        &self,
        node: &str,
        cpu: f64,
        mem_used: u64,
        mem_total: u64,
        swap_used: u64,
        swap_total: u64,
        disk_used: u64,
        disk_total: u64,
        load_avg1: f64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO node_metrics (node, cpu, mem_used, mem_total, swap_used, swap_total, disk_used, disk_total, load_avg1)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![node, cpu, mem_used, mem_total, swap_used, swap_total, disk_used, disk_total, load_avg1],
        )?;
        Ok(())
    }

    pub fn insert_guest_metric(
        &self,
        vmid: u32,
        name: &str,
        kind: &str,
        status: &str,
        cpu: f64,
        mem_used: u64,
        mem_total: u64,
        node: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO guest_metrics (vmid, name, kind, status, cpu, mem_used, mem_total, node)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![vmid, name, kind, status, cpu, mem_used, mem_total, node],
        )?;
        Ok(())
    }

    #[allow(dead_code)]
    pub fn insert_log_line(&self, source: &str, severity: &str, line: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO log_lines (source, severity, line) VALUES (?1, ?2, ?3)",
            params![source, severity, line],
        )?;
        Ok(())
    }

    pub fn insert_alert(&self, alert_key: &str, severity: &str, summary: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO alert_history (alert_key, severity, summary) VALUES (?1, ?2, ?3)",
            params![alert_key, severity, summary],
        )?;
        Ok(())
    }

    pub fn insert_haproxy_metric(
        &self,
        proxy_name: &str,
        server_name: &str,
        status: &str,
        sessions: u64,
        bytes_in: u64,
        bytes_out: u64,
        http_5xx: u64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO haproxy_metrics (proxy_name, server_name, status, sessions, bytes_in, bytes_out, http_5xx)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![proxy_name, server_name, status, sessions, bytes_in, bytes_out, http_5xx],
        )?;
        Ok(())
    }

    // ── Query functions ──────────────────────────────────────────────────────

    /// Get node metrics for the last N hours
    pub fn query_node_history(
        &self,
        node: &str,
        hours: u32,
    ) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT ts, cpu, mem_used, mem_total, swap_used, swap_total, disk_used, disk_total, load_avg1
             FROM node_metrics
             WHERE node = ?1
               AND ts >= strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?2)
             ORDER BY ts ASC"
        )?;

        let time_offset = format!("-{hours} hours");
        let rows = stmt.query_map(params![node, time_offset], |row| {
            Ok(serde_json::json!({
                "ts": row.get::<_, String>(0)?,
                "cpu": row.get::<_, f64>(1)?,
                "mem_used": row.get::<_, u64>(2)?,
                "mem_total": row.get::<_, u64>(3)?,
                "swap_used": row.get::<_, u64>(4)?,
                "swap_total": row.get::<_, u64>(5)?,
                "disk_used": row.get::<_, u64>(6)?,
                "disk_total": row.get::<_, u64>(7)?,
                "load_avg1": row.get::<_, f64>(8)?
            }))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    /// Get recent alerts
    pub fn query_recent_alerts(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT ts, alert_key, severity, summary, resolved
             FROM alert_history
             ORDER BY ts DESC
             LIMIT ?1"
        )?;

        let rows = stmt.query_map(params![limit], |row| {
            Ok(serde_json::json!({
                "ts": row.get::<_, String>(0)?,
                "alert_key": row.get::<_, String>(1)?,
                "severity": row.get::<_, String>(2)?,
                "summary": row.get::<_, String>(3)?,
                "resolved": row.get::<_, bool>(4)?
            }))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    // ── Retention cleanup ────────────────────────────────────────────────────

    /// Delete data older than the configured retention periods
    pub fn cleanup_old_data(
        &self,
        metric_retention_days: u32,
        log_retention_days: u32,
        alert_retention_days: u32,
    ) -> Result<CleanupStats> {
        let conn = self.conn.lock().unwrap();

        let metric_offset = format!("-{metric_retention_days} days");
        let log_offset = format!("-{log_retention_days} days");
        let alert_offset = format!("-{alert_retention_days} days");

        let node_deleted = conn.execute(
            "DELETE FROM node_metrics WHERE ts < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)",
            params![metric_offset],
        )?;

        let guest_deleted = conn.execute(
            "DELETE FROM guest_metrics WHERE ts < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)",
            params![metric_offset],
        )?;

        let haproxy_deleted = conn.execute(
            "DELETE FROM haproxy_metrics WHERE ts < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)",
            params![metric_offset],
        )?;

        let log_deleted = conn.execute(
            "DELETE FROM log_lines WHERE ts < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)",
            params![log_offset],
        )?;

        let alert_deleted = conn.execute(
            "DELETE FROM alert_history WHERE ts < strftime('%Y-%m-%dT%H:%M:%SZ', 'now', ?1)",
            params![alert_offset],
        )?;

        Ok(CleanupStats {
            node_metrics: node_deleted,
            guest_metrics: guest_deleted,
            haproxy_metrics: haproxy_deleted,
            log_lines: log_deleted,
            alerts: alert_deleted,
        })
    }
}

#[derive(Debug)]
pub struct CleanupStats {
    pub node_metrics: usize,
    pub guest_metrics: usize,
    pub haproxy_metrics: usize,
    pub log_lines: usize,
    pub alerts: usize,
}

impl std::fmt::Display for CleanupStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cleaned up: {} node, {} guest, {} haproxy metrics, {} logs, {} alerts",
            self.node_metrics, self.guest_metrics, self.haproxy_metrics,
            self.log_lines, self.alerts
        )
    }
}
