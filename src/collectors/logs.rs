// src/collectors/logs.rs
//
// Real-time log collection using inotify.
// For LXCs: reads the container rootfs logs directly from the host.
// For VMs: tails via SSH in a background task.
//
// Parsed log lines are:
//   - Stored in a ring buffer (last N per source)
//   - Matched against alert patterns
//   - Exported as Prometheus counters per severity/pattern

use anyhow::Result;
use notify::{Config as NotifyConfig, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, warn};

use crate::config::{AlertSeverity, LogAlertPattern, LogConfig};

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct LogLine {
    pub source: String,         // "lxc:101:/var/log/syslog"
    pub path: String,           // actual host path
    pub line: String,
    pub timestamp_ms: i64,      // wall clock when we read it
    pub pattern_match: Option<String>,
    pub severity: Option<AlertSeverity>,
}

/// Shared log buffer — last N lines per source
pub type LogBuffer = Arc<Mutex<HashMap<String, VecDeque<LogLine>>>>;

/// Alert channel
pub type AlertTx = mpsc::UnboundedSender<LogAlert>;

#[derive(Debug, Clone, Serialize)]
pub struct LogAlert {
    pub source: String,
    pub pattern: String,
    pub severity: AlertSeverity,
    pub line: String,
    pub timestamp_ms: i64,
}

// ──────────────────────────────────────────────────────────────────────────────
// Compiled patterns
// ──────────────────────────────────────────────────────────────────────────────

struct CompiledPattern {
    name: String,
    re: Regex,
    severity: AlertSeverity,
}

fn compile_patterns(patterns: &[LogAlertPattern]) -> Vec<CompiledPattern> {
    patterns
        .iter()
        .filter_map(|p| {
            Regex::new(&p.pattern)
                .map(|re| CompiledPattern {
                    name: p.name.clone(),
                    re,
                    severity: p.severity.clone(),
                })
                .map_err(|e| warn!("Invalid regex '{}': {e}", p.pattern))
                .ok()
        })
        .collect()
}

// ──────────────────────────────────────────────────────────────────────────────
// Log collector
// ──────────────────────────────────────────────────────────────────────────────

pub struct LogCollector {
    cfg: LogConfig,
    buffer: LogBuffer,
    alert_tx: AlertTx,
    patterns: Arc<Vec<CompiledPattern>>,
    ws_tx: Option<broadcast::Sender<String>>,
}

impl LogCollector {
    pub fn new(cfg: LogConfig, alert_tx: AlertTx, ws_tx: Option<broadcast::Sender<String>>) -> Self {
        let patterns = compile_patterns(&cfg.alert_patterns);
        Self {
            cfg,
            buffer: Arc::new(Mutex::new(HashMap::new())),
            alert_tx,
            patterns: Arc::new(patterns),
            ws_tx,
        }
    }

    pub fn buffer(&self) -> LogBuffer {
        self.buffer.clone()
    }

    /// Register an LXC log file for watching.
    /// `vmid` = LXC id, `log_path` = path inside container (e.g. /var/log/syslog)
    pub async fn watch_lxc_log(&self, vmid: u32, log_path: &str) -> Result<()> {
        let host_path = PathBuf::from(format!(
            "/var/lib/lxc/{}/rootfs{}",
            vmid, log_path
        ));

        if !host_path.exists() {
            debug!("Log not found (LXC may not have it): {}", host_path.display());
            return Ok(());
        }

        let source = format!("lxc:{vmid}:{log_path}");
        info!("Watching LXC log: {}", host_path.display());

        // Read initial tail
        self.initial_tail(&host_path, &source).await;

        // Start inotify watcher
        self.spawn_watcher(host_path, source);
        Ok(())
    }

    /// Register a host-level log file (e.g. /var/log/pve/tasks on the Proxmox host)
    pub async fn watch_host_log(&self, log_path: &str) -> Result<()> {
        let path = PathBuf::from(log_path);
        if !path.exists() {
            return Ok(());
        }
        let source = format!("host:{log_path}");
        self.initial_tail(&path, &source).await;
        self.spawn_watcher(path, source);
        Ok(())
    }

    async fn initial_tail(&self, path: &Path, source: &str) {
        match tokio::process::Command::new("tail")
            .args(["-n", &self.cfg.tail_lines.to_string()])
            .arg(path)
            .output()
            .await
        {
            Ok(out) => {
                let content = String::from_utf8_lossy(&out.stdout);
                for line in content.lines() {
                    self.ingest_line(source, path.to_str().unwrap_or(""), line);
                }
            }
            Err(e) => warn!("tail {}: {e}", path.display()),
        }
    }

    /// Spawn a background inotify watcher for a single file.
    fn spawn_watcher(&self, host_path: PathBuf, source: String) {
        let buffer = self.buffer.clone();
        let patterns = self.patterns.clone();
        let alert_tx = self.alert_tx.clone();
        let buf_size = self.cfg.buffer_size;
        let ws_tx = self.ws_tx.clone();

        tokio::task::spawn_blocking(move || {
            // notify uses std channels; bridge to tokio via std thread
            let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
            let mut watcher =
                RecommendedWatcher::new(tx, NotifyConfig::default()).expect("inotify");
            watcher
                .watch(&host_path, RecursiveMode::NonRecursive)
                .expect("watch");

            // We need to read new bytes when the file grows.
            // Track position for incremental reads.
            let mut pos: u64 = std::fs::metadata(&host_path).map(|m| m.len()).unwrap_or(0);
            let path_str = host_path.to_str().unwrap_or("").to_string();

            for event in rx {
                match event {
                    Ok(Event { kind: EventKind::Modify(_), .. }) => {
                        // Read new bytes since last position
                        if let Ok(mut f) = std::fs::File::open(&host_path) {
                            use std::io::{Read, Seek, SeekFrom};
                            let new_len = f.metadata().map(|m| m.len()).unwrap_or(0);
                            if new_len < pos {
                                // Log rotated — reset
                                pos = 0;
                            }
                            if f.seek(SeekFrom::Start(pos)).is_ok() {
                                let mut buf = String::new();
                                if f.read_to_string(&mut buf).is_ok() {
                                    pos = new_len;
                                    for line in buf.lines() {
                                        let ts = chrono::Utc::now().timestamp_millis();
                                        // Pattern match
                                        let mut matched_name = None;
                                        let mut matched_sev = None;
                                        for pat in patterns.iter() {
                                            if pat.re.is_match(line) {
                                                matched_name = Some(pat.name.clone());
                                                matched_sev = Some(pat.severity.clone());
                                                // Send alert
                                                let _ = alert_tx.send(LogAlert {
                                                    source: source.clone(),
                                                    pattern: pat.name.clone(),
                                                    severity: pat.severity.clone(),
                                                    line: line.to_string(),
                                                    timestamp_ms: ts,
                                                });
                                                break; // first match wins
                                            }
                                        }
                                        // Store in ring buffer
                                        let sev_str = matched_sev.as_ref().map(|s| s.as_str()).unwrap_or("info");
                                        let log_line = LogLine {
                                            source: source.clone(),
                                            path: path_str.clone(),
                                            line: line.to_string(),
                                            timestamp_ms: ts,
                                            pattern_match: matched_name,
                                            severity: matched_sev,
                                        };
                                        let mut guard = buffer.lock().unwrap();
                                        let ring = guard
                                            .entry(source.clone())
                                            .or_insert_with(VecDeque::new);
                                        if ring.len() >= buf_size {
                                            ring.pop_front();
                                        }
                                        ring.push_back(log_line);

                                        // Broadcast to WebSocket clients
                                        if let Some(ref tx) = ws_tx {
                                            let event = serde_json::json!({
                                                "type": "log_line",
                                                "source": source,
                                                "severity": sev_str,
                                                "line": line,
                                                "ts": ts
                                            });
                                            let _ = tx.send(event.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Ok(Event { kind: EventKind::Remove(_), .. }) => {
                        // Log rotated away — re-watch when it reappears
                        info!("Log removed, will re-watch: {}", host_path.display());
                        // Simple: sleep and retry
                        std::thread::sleep(std::time::Duration::from_secs(5));
                        if host_path.exists() {
                            pos = 0;
                            watcher
                                .watch(&host_path, RecursiveMode::NonRecursive)
                                .ok();
                        }
                    }
                    Err(e) => warn!("inotify error: {e}"),
                    _ => {}
                }
            }
        });
    }

    fn ingest_line(&self, source: &str, path: &str, line: &str) {
        let ts = chrono::Utc::now().timestamp_millis();
        let mut matched_name = None;
        let mut matched_sev = None;

        for pat in self.patterns.iter() {
            if pat.re.is_match(line) {
                matched_name = Some(pat.name.clone());
                matched_sev = Some(pat.severity.clone());
                break;
            }
        }

        let log_line = LogLine {
            source: source.to_string(),
            path: path.to_string(),
            line: line.to_string(),
            timestamp_ms: ts,
            pattern_match: matched_name,
            severity: matched_sev,
        };

        let mut guard = self.buffer.lock().unwrap();
        let ring = guard
            .entry(source.to_string())
            .or_insert_with(VecDeque::new);
        if ring.len() >= self.cfg.buffer_size {
            ring.pop_front();
        }
        ring.push_back(log_line);
    }

    /// Get last N log lines for a specific source
    pub fn recent_lines(&self, source: &str, n: usize) -> Vec<LogLine> {
        let guard = self.buffer.lock().unwrap();
        guard
            .get(source)
            .map(|ring| ring.iter().rev().take(n).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    }

    /// Get all sources being watched
    pub fn sources(&self) -> Vec<String> {
        self.buffer.lock().unwrap().keys().cloned().collect()
    }

    /// Get alert-level lines across all sources (last N)
    pub fn recent_alerts(&self, n: usize) -> Vec<LogLine> {
        let guard = self.buffer.lock().unwrap();
        let mut alerts: Vec<LogLine> = guard
            .values()
            .flat_map(|ring| ring.iter())
            .filter(|l| l.severity.is_some())
            .cloned()
            .collect();
        alerts.sort_by_key(|l| std::cmp::Reverse(l.timestamp_ms));
        alerts.truncate(n);
        alerts
    }
}



// ──────────────────────────────────────────────────────────────────────────────
// Commonly-watched Proxmox host logs
// ──────────────────────────────────────────────────────────────────────────────

pub const PROXMOX_HOST_LOGS: &[&str] = &[
    "/var/log/pve/tasks/active",
    "/var/log/daemon.log",
    "/var/log/auth.log",
    "/var/log/syslog",
    "/var/log/kern.log",
];

/// Standard logs to watch inside LXC/VM containers
pub const CONTAINER_LOGS: &[&str] = &[
    "/var/log/syslog",
    "/var/log/auth.log",
    "/var/log/apache2/error.log",
    "/var/log/apache2/access.log",
    "/var/log/nginx/error.log",
    "/var/log/nginx/access.log",
    "/var/log/php-fpm.log",
    "/var/log/php8.2-fpm.log",
    "/var/log/php8.3-fpm.log",
    "/var/log/mysql/error.log",
    "/var/log/postgresql/postgresql.log",
    "/var/log/redis/redis-server.log",
];
