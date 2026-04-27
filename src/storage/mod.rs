// src/storage/mod.rs
//
// Persistent storage via embedded SQLite.
// Stores metric snapshots, log lines, and alert history
// with configurable retention periods.

use crate::config::AlertRuleConfig;
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
             PRAGMA busy_timeout = 5000;",
        )?;

        let storage = Self {
            conn: Mutex::new(conn),
        };
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

            -- Application metrics snapshots
            CREATE TABLE IF NOT EXISTS app_metrics (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                ts         TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                app_name   TEXT    NOT NULL,
                metric     TEXT    NOT NULL,
                value      REAL    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_app_metrics_ts ON app_metrics(ts);
            CREATE INDEX IF NOT EXISTS idx_app_metrics_app ON app_metrics(app_name);

            -- UI-created custom alert rules. Static rules still live in config.toml.
            CREATE TABLE IF NOT EXISTS alert_rules_ui (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                rule_json  TEXT NOT NULL
            );
            ",
        )
        .context("Creating database tables")?;
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
            params![
                node,
                cpu,
                sqlite_i64(mem_used),
                sqlite_i64(mem_total),
                sqlite_i64(swap_used),
                sqlite_i64(swap_total),
                sqlite_i64(disk_used),
                sqlite_i64(disk_total),
                load_avg1
            ],
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
            params![
                vmid,
                name,
                kind,
                status,
                cpu,
                sqlite_i64(mem_used),
                sqlite_i64(mem_total),
                node
            ],
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
            params![
                proxy_name,
                server_name,
                status,
                sqlite_i64(sessions),
                sqlite_i64(bytes_in),
                sqlite_i64(bytes_out),
                sqlite_i64(http_5xx)
            ],
        )?;
        Ok(())
    }

    pub fn insert_app_metric(&self, app_name: &str, metric: &str, value: f64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO app_metrics (app_name, metric, value) VALUES (?1, ?2, ?3)",
            params![app_name, metric, value],
        )?;
        Ok(())
    }

    // ── Query functions ──────────────────────────────────────────────────────

    /// Get node metrics for the last N hours
    pub fn query_node_history(&self, node: &str, hours: u32) -> Result<Vec<serde_json::Value>> {
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
                "mem_used": sqlite_u64(row.get::<_, i64>(2)?),
                "mem_total": sqlite_u64(row.get::<_, i64>(3)?),
                "swap_used": sqlite_u64(row.get::<_, i64>(4)?),
                "swap_total": sqlite_u64(row.get::<_, i64>(5)?),
                "disk_used": sqlite_u64(row.get::<_, i64>(6)?),
                "disk_total": sqlite_u64(row.get::<_, i64>(7)?),
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
             LIMIT ?1",
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

    pub fn query_app_history(
        &self,
        app_name: &str,
        metric: &str,
        minutes: u32,
    ) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT ts, value FROM app_metrics
             WHERE app_name = ?1 AND metric = ?2
               AND ts > datetime('now', '-' || ?3 || ' minutes')
             ORDER BY ts ASC",
        )?;

        let rows = stmt.query_map(params![app_name, metric, minutes], |row| {
            Ok(serde_json::json!({
                "ts": row.get::<_, String>(0)?,
                "value": row.get::<_, f64>(1)?
            }))
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn query_ui_alert_rule_configs(&self) -> Result<Vec<AlertRuleConfig>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT rule_json FROM alert_rules_ui ORDER BY id ASC")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;

        let mut rules = Vec::new();
        for row in rows {
            let raw = row?;
            match serde_json::from_str::<AlertRuleConfig>(&raw) {
                Ok(rule) => rules.push(rule),
                Err(e) => tracing::warn!("Ignoring invalid UI alert rule JSON: {e}"),
            }
        }
        Ok(rules)
    }

    pub fn query_ui_alert_rules(&self) -> Result<Vec<serde_json::Value>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, created_at, updated_at, rule_json FROM alert_rules_ui ORDER BY id ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            let id = row.get::<_, i64>(0)?;
            let created_at = row.get::<_, String>(1)?;
            let updated_at = row.get::<_, String>(2)?;
            let raw = row.get::<_, String>(3)?;
            let mut value: serde_json::Value =
                serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({}));
            if let Some(obj) = value.as_object_mut() {
                obj.insert("id".to_string(), serde_json::json!(id));
                obj.insert("source".to_string(), serde_json::json!("ui"));
                obj.insert("created_at".to_string(), serde_json::json!(created_at));
                obj.insert("updated_at".to_string(), serde_json::json!(updated_at));
            }
            Ok(value)
        })?;

        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }

    pub fn insert_ui_alert_rule(&self, rule: &AlertRuleConfig) -> Result<i64> {
        rule.validate()?;
        let raw = serde_json::to_string(rule)?;
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO alert_rules_ui (rule_json) VALUES (?1)",
            params![raw],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_ui_alert_rule(&self, id: i64, rule: &AlertRuleConfig) -> Result<()> {
        rule.validate()?;
        let raw = serde_json::to_string(rule)?;
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute(
            "UPDATE alert_rules_ui
             SET rule_json = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
             WHERE id = ?2",
            params![raw, id],
        )?;
        if changed == 0 {
            anyhow::bail!("alert rule {id} not found");
        }
        Ok(())
    }

    pub fn delete_ui_alert_rule(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let changed = conn.execute("DELETE FROM alert_rules_ui WHERE id = ?1", params![id])?;
        if changed == 0 {
            anyhow::bail!("alert rule {id} not found");
        }
        Ok(())
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
            self.node_metrics,
            self.guest_metrics,
            self.haproxy_metrics,
            self.log_lines,
            self.alerts
        )
    }
}

fn sqlite_i64(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}

fn sqlite_u64(value: i64) -> u64 {
    u64::try_from(value).unwrap_or(0)
}
