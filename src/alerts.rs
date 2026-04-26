// src/alerts.rs
//
// Receives alert events and dispatches them to:
//   - Webhook (Alertmanager / Grafana OnCall / Slack-compatible)
//   - Structured log output (always)
//
// Implements basic deduplication: same alert won't re-fire within
// a configurable silence window (default: 5 minutes).

use once_cell::sync::Lazy;
use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

use tracing::{error, info, warn};

use crate::alert_channels::{AlertChannel, AlertNotification};
use crate::collectors::logs::LogAlert;
use crate::config::{AlertConfig, AlertSeverity};
use crate::proxmox_api::{GuestStatus, NodeStatus};
use crate::storage::Storage;

const SILENCE_WINDOW: Duration = Duration::from_secs(300); // 5 min dedup
static SHARED_SILENCE_MAP: Lazy<Arc<Mutex<HashMap<String, Instant>>>> =
    Lazy::new(|| Arc::new(Mutex::new(HashMap::new())));

// ──────────────────────────────────────────────────────────────────────────────
// Alert events
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Alert {
    NodeHighCpu {
        node: String,
        cpu_pct: f64,
    },
    NodeHighMemory {
        node: String,
        mem_pct: f64,
    },
    NodeHighDisk {
        node: String,
        disk_pct: f64,
    },
    GuestDown {
        vmid: u32,
        name: String,
        node: String,
    },
    GuestHighCpu {
        vmid: u32,
        name: String,
        cpu_pct: f64,
    },
    GuestHighMemory {
        vmid: u32,
        name: String,
        mem_pct: f64,
    },
    DiskFull {
        vmid: u32,
        name: String,
        mountpoint: String,
        use_pct: f64,
    },
    LogPattern(LogAlert),
    StorageUnavailable {
        storage: String,
        node: String,
    },
    HaproxyBackendDown {
        proxy: String,
        server: String,
        duration_secs: u64,
    },
    ServiceUnavailable {
        vmid: u32,
        node: String,
        service: String,
    },
    PostgresDown {
        url: String,
        _error: String,
    },
    RedisDown {
        url: String,
        _error: String,
    },
    S3Degraded {
        endpoint: String,
        bucket: String,
        _error: String,
    },
    MigrationDetected {
        vmid: u32,
        name: String,
        from_node: String,
        to_node: String,
    },
    NodePressureCritical {
        node: String,
        mem_pct: f64,
        suggest_vmid: Option<u32>,
        target_node: Option<String>,
    },
    VmConnectionLost {
        vmid: u32,
        name: String,
        node: String,
    },
    OomKilled {
        node: String,
        process: String,
    },
    AppDown {
        name: String,
    },
    AppHighErrorRate {
        name: String,
        error_rate: f64,
    },
    AppAuthFailures {
        name: String,
        count: u64,
    },
    #[allow(dead_code)]
    AppStorageFull {
        name: String,
        usage_pct: f64,
    },
    #[allow(dead_code)]
    AppVersionMismatch {
        name: String,
        expected: String,
        found: String,
    },
    PlatformIssue {
        key: String,
        severity: String,
        summary: String,
    },
    CustomRuleTriggered {
        name: String,
        scope: String,
        severity: String,
        summary: String,
    },
    Test {
        message: String,
    },
}

impl Alert {
    pub fn key(&self) -> String {
        match self {
            Alert::NodeHighCpu { node, .. } => format!("node_cpu:{node}"),
            Alert::NodeHighMemory { node, .. } => format!("node_mem:{node}"),
            Alert::NodeHighDisk { node, .. } => format!("node_disk:{node}"),
            Alert::GuestDown { vmid, .. } => format!("guest_down:{vmid}"),
            Alert::GuestHighCpu { vmid, .. } => format!("guest_cpu:{vmid}"),
            Alert::GuestHighMemory { vmid, .. } => format!("guest_mem:{vmid}"),
            Alert::DiskFull {
                vmid, mountpoint, ..
            } => format!("disk_full:{vmid}:{mountpoint}"),
            Alert::LogPattern(l) => format!("log:{}:{}", l.source, l.pattern),
            Alert::StorageUnavailable { storage, node } => format!("storage:{node}:{storage}"),
            Alert::HaproxyBackendDown { proxy, server, .. } => {
                format!("haproxy_down:{proxy}:{server}")
            }
            Alert::ServiceUnavailable { vmid, service, .. } => {
                format!("service_down:{vmid}:{service}")
            }
            Alert::PostgresDown { url, .. } => format!("postgres_down:{url}"),
            Alert::RedisDown { url, .. } => format!("redis_down:{url}"),
            Alert::S3Degraded {
                endpoint, bucket, ..
            } => format!("s3_degraded:{endpoint}:{bucket}"),
            Alert::MigrationDetected {
                vmid,
                from_node,
                to_node,
                ..
            } => format!("migration:{vmid}:{from_node}:{to_node}"),
            Alert::NodePressureCritical { node, .. } => format!("node_pressure:{node}"),
            Alert::VmConnectionLost { vmid, .. } => format!("vm_conn_lost:{vmid}"),
            Alert::OomKilled { node, process } => format!("oom_killed:{node}:{process}"),
            Alert::AppDown { name } => format!("app_down:{name}"),
            Alert::AppHighErrorRate { name, .. } => format!("app_errors:{name}"),
            Alert::AppAuthFailures { name, .. } => format!("app_auth:{name}"),
            Alert::AppStorageFull { name, .. } => format!("app_storage:{name}"),
            Alert::AppVersionMismatch { name, .. } => format!("app_version:{name}"),
            Alert::PlatformIssue { key, .. } => format!("platform:{key}"),
            Alert::CustomRuleTriggered { name, scope, .. } => format!("custom_rule:{name}:{scope}"),
            Alert::Test { .. } => format!("test_alert:{}", chrono::Utc::now().timestamp()),
        }
    }

    pub fn severity(&self) -> &'static str {
        match self {
            Alert::GuestDown { .. }
            | Alert::StorageUnavailable { .. }
            | Alert::HaproxyBackendDown { .. }
            | Alert::ServiceUnavailable { .. }
            | Alert::PostgresDown { .. }
            | Alert::RedisDown { .. }
            | Alert::S3Degraded { .. }
            | Alert::NodePressureCritical { .. }
            | Alert::AppDown { .. }
            | Alert::AppHighErrorRate { .. }
            | Alert::AppStorageFull { .. }
            | Alert::OomKilled { .. } => "critical",
            Alert::VmConnectionLost { .. } => "warning",
            Alert::PlatformIssue { severity, .. } => match severity.as_str() {
                "critical" => "critical",
                "info" => "info",
                _ => "warning",
            },
            Alert::CustomRuleTriggered { severity, .. } => match severity.as_str() {
                "critical" => "critical",
                "info" => "info",
                _ => "warning",
            },
            Alert::MigrationDetected { .. } => "info",
            Alert::NodeHighCpu { cpu_pct, .. } | Alert::GuestHighCpu { cpu_pct, .. }
                if *cpu_pct > 95.0 =>
            {
                "critical"
            }
            Alert::NodeHighDisk { disk_pct, .. }
            | Alert::DiskFull {
                use_pct: disk_pct, ..
            } if *disk_pct > 95.0 => "critical",
            Alert::LogPattern(l) => match l.severity {
                AlertSeverity::Critical => "critical",
                AlertSeverity::Warning => "warning",
                AlertSeverity::Info => "info",
            },
            _ => "warning",
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Alert::NodeHighCpu { node, cpu_pct } => format!("Node {node} CPU at {cpu_pct:.1}%"),
            Alert::NodeHighMemory { node, mem_pct } => {
                format!("Node {node} memory at {mem_pct:.1}%")
            }
            Alert::NodeHighDisk { node, disk_pct } => format!("Node {node} disk at {disk_pct:.1}%"),
            Alert::GuestDown { name, vmid, node } => {
                format!("Guest {name} (vmid {vmid}) down on {node}")
            }
            Alert::GuestHighCpu {
                name,
                vmid,
                cpu_pct,
            } => format!("Guest {name} ({vmid}) CPU at {cpu_pct:.1}%"),
            Alert::GuestHighMemory {
                name,
                vmid,
                mem_pct,
            } => format!("Guest {name} ({vmid}) memory at {mem_pct:.1}%"),
            Alert::DiskFull {
                name,
                mountpoint,
                use_pct,
                ..
            } => format!("Disk {mountpoint} on {name} at {use_pct:.1}%"),
            Alert::LogPattern(l) => format!(
                "[{}] {} matched '{}': {}",
                l.severity.as_str(),
                l.source,
                l.pattern,
                &l.line[..l.line.len().min(100)]
            ),
            Alert::StorageUnavailable { storage, node } => {
                format!("Storage {storage} unavailable on {node}")
            }
            Alert::HaproxyBackendDown {
                proxy,
                server,
                duration_secs,
            } => format!("HAProxy {proxy}/{server} DOWN for {duration_secs}s"),
            Alert::ServiceUnavailable {
                vmid,
                node,
                service,
            } => format!("Critical service '{service}' is DOWN on VM {vmid} ({node})"),
            Alert::PostgresDown { url, _error } => format!("PostgreSQL down at {url}: {_error}"),
            Alert::RedisDown { url, _error } => format!("Redis down at {url}: {_error}"),
            Alert::S3Degraded {
                endpoint,
                bucket,
                _error,
            } => format!("S3 degraded at {endpoint} bucket {bucket}: {_error}"),
            Alert::MigrationDetected {
                vmid,
                name,
                from_node,
                to_node,
            } => format!("VM {name} ({vmid}) migrated from {from_node} to {to_node}"),
            Alert::NodePressureCritical {
                node,
                mem_pct,
                suggest_vmid,
                target_node,
            } => {
                if let (Some(vmid), Some(target)) = (suggest_vmid, target_node) {
                    format!(
                        "Node {node} pressure critical ({mem_pct:.1}% mem). Migration suggested: `qm migrate {vmid} {target} --online`"
                    )
                } else {
                    format!("Node {node} pressure critical ({mem_pct:.1}% mem).")
                }
            }
            Alert::VmConnectionLost { vmid, name, node } => {
                format!("VM {name} ({vmid}) on {node} lost connection (agent/ssh)")
            }
            Alert::OomKilled { node, process } => {
                format!("OOM Killer triggered on {node} for process '{process}'")
            }
            Alert::AppDown { name } => format!("Application {name} is DOWN or unreachable"),
            Alert::AppHighErrorRate { name, error_rate } => {
                format!("Application {name} high error rate: {error_rate:.1}/min")
            }
            Alert::AppAuthFailures { name, count } => format!(
                "Application {name} detected {count} authentication failures in the last minute"
            ),
            Alert::AppStorageFull { name, usage_pct } => {
                format!("Application {name} storage nearly full: {usage_pct:.1}%")
            }
            Alert::AppVersionMismatch {
                name,
                expected,
                found,
            } => format!("Application {name} version mismatch: expected {expected}, found {found}"),
            Alert::PlatformIssue { summary, .. } => summary.clone(),
            Alert::CustomRuleTriggered { summary, .. } => summary.clone(),
            Alert::Test { message } => format!("SENTINEL TEST ALERT: {message}"),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Alert dispatcher
// ──────────────────────────────────────────────────────────────────────────────

pub struct AlertDispatcher {
    cfg: AlertConfig,
    http: Option<Client>,
    channels: Vec<AlertChannel>,
    silence_map: Arc<Mutex<HashMap<String, Instant>>>,
    storage: Option<Arc<Storage>>,
}

impl AlertDispatcher {
    pub fn new(mut cfg: AlertConfig, storage: Option<Arc<Storage>>) -> Self {
        if cfg
            .webhook_url
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            cfg.webhook_url = None;
        }
        let mut channels = Vec::new();
        if let Some(url) = cfg.webhook_url.clone() {
            channels.push(AlertChannel::webhook(url));
        }
        let http = if cfg.enabled && !channels.is_empty() {
            Some(
                Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .expect("HTTP client"),
            )
        } else {
            None
        };
        Self {
            cfg,
            http,
            channels,
            silence_map: SHARED_SILENCE_MAP.clone(),
            storage,
        }
    }

    pub async fn dispatch(&mut self, alert: Alert) {
        let key = alert.key();
        let summary = alert.summary();
        let severity = alert.severity();

        // Deduplication is intentionally shared across all dispatcher instances.
        // Collectors run in separate Tokio tasks, but duplicate alert keys should
        // still silence globally instead of once per collector.
        {
            let mut silence_map = self.silence_map.lock().await;
            if let Some(last) = silence_map.get(&key) {
                if last.elapsed() < SILENCE_WINDOW {
                    return;
                }
            }
            silence_map.insert(key.clone(), Instant::now());
        }

        // Always log
        match severity {
            "critical" => error!("ALERT [critical] {summary}"),
            "warning" => warn!("ALERT [warning] {summary}"),
            _ => info!("ALERT [info] {summary}"),
        }

        // Save to SQLite
        if let Some(ref storage) = self.storage {
            if let Err(e) = storage.insert_alert(&key, severity, &summary) {
                warn!("Failed to save alert to history: {}", e);
            }
        }

        // Alert channel dispatch
        if self.cfg.enabled {
            if let Some(ref client) = self.http {
                let notification = AlertNotification {
                    key,
                    severity: severity.to_string(),
                    summary,
                };
                for channel in &self.channels {
                    if let Err(e) = channel.send(client, &notification).await {
                        error!("Alert channel '{}' failed: {e}", channel.name());
                    }
                }
            }
        }
    }

    /// Check node thresholds and generate alerts
    pub fn check_node(&self, n: &NodeStatus) -> Vec<Alert> {
        let mut alerts = Vec::new();

        let cpu_pct = n.cpu_usage * 100.0;
        if cpu_pct > self.cfg.cpu_threshold {
            alerts.push(Alert::NodeHighCpu {
                node: n.node.clone(),
                cpu_pct,
            });
        }

        if n.mem_total > 0 {
            let mem_pct = (n.mem_used as f64 / n.mem_total as f64) * 100.0;
            if mem_pct > self.cfg.memory_threshold {
                alerts.push(Alert::NodeHighMemory {
                    node: n.node.clone(),
                    mem_pct,
                });
            }
        }

        if n.disk_total > 0 {
            let disk_pct = (n.disk_used as f64 / n.disk_total as f64) * 100.0;
            if disk_pct > self.cfg.disk_threshold {
                alerts.push(Alert::NodeHighDisk {
                    node: n.node.clone(),
                    disk_pct,
                });
            }
        }

        alerts
    }

    /// Check guest thresholds
    pub fn check_guest(&self, g: &GuestStatus) -> Vec<Alert> {
        let mut alerts = Vec::new();

        if g.status != "running" {
            alerts.push(Alert::GuestDown {
                vmid: g.vmid,
                name: g.name.clone(),
                node: g.node.clone(),
            });
            return alerts; // no point checking resources if down
        }

        let cpu_pct = g.cpu_usage * 100.0;
        if cpu_pct > self.cfg.cpu_threshold {
            alerts.push(Alert::GuestHighCpu {
                vmid: g.vmid,
                name: g.name.clone(),
                cpu_pct,
            });
        }

        if g.mem_total > 0 {
            let mem_pct = (g.mem_used as f64 / g.mem_total as f64) * 100.0;
            if mem_pct > self.cfg.memory_threshold {
                alerts.push(Alert::GuestHighMemory {
                    vmid: g.vmid,
                    name: g.name.clone(),
                    mem_pct,
                });
            }
        }

        alerts
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Threshold checker for disk mounts inside containers
// ──────────────────────────────────────────────────────────────────────────────

pub fn check_disk_threshold(
    vmid: u32,
    name: &str,
    mountpoint: &str,
    use_pct: f64,
    threshold: f64,
) -> Option<Alert> {
    if use_pct > threshold {
        Some(Alert::DiskFull {
            vmid,
            name: name.to_string(),
            mountpoint: mountpoint.to_string(),
            use_pct,
        })
    } else {
        None
    }
}
