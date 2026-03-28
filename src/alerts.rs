// src/alerts.rs
//
// Receives alert events and dispatches them to:
//   - Webhook (Alertmanager / Grafana OnCall / Slack-compatible)
//   - Structured log output (always)
//
// Implements basic deduplication: same alert won't re-fire within
// a configurable silence window (default: 5 minutes).

use reqwest::Client;
use serde::Serialize;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use tracing::{error, info, warn};

use crate::collectors::logs::LogAlert;
use crate::config::{AlertConfig, AlertSeverity};
use crate::proxmox_api::{GuestStatus, NodeStatus};
use crate::storage::Storage;
use std::sync::Arc;

const SILENCE_WINDOW: Duration = Duration::from_secs(300); // 5 min dedup

// ──────────────────────────────────────────────────────────────────────────────
// Alert events
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Alert {
    NodeHighCpu { node: String, cpu_pct: f64 },
    NodeHighMemory { node: String, mem_pct: f64 },
    NodeHighDisk { node: String, disk_pct: f64 },
    GuestDown { vmid: u32, name: String, node: String },
    GuestHighCpu { vmid: u32, name: String, cpu_pct: f64 },
    GuestHighMemory { vmid: u32, name: String, mem_pct: f64 },
    DiskFull { vmid: u32, name: String, mountpoint: String, use_pct: f64 },
    LogPattern(LogAlert),
    StorageUnavailable { storage: String, node: String },
    HaproxyBackendDown { proxy: String, server: String, duration_secs: u64 },
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
            Alert::DiskFull { vmid, mountpoint, .. } => format!("disk_full:{vmid}:{mountpoint}"),
            Alert::LogPattern(l) => format!("log:{}:{}", l.source, l.pattern),
            Alert::StorageUnavailable { storage, node } => format!("storage:{node}:{storage}"),
            Alert::HaproxyBackendDown { proxy, server, .. } => format!("haproxy_down:{proxy}:{server}"),
        }
    }

    pub fn severity(&self) -> &'static str {
        match self {
            Alert::GuestDown { .. }
            | Alert::StorageUnavailable { .. }
            | Alert::HaproxyBackendDown { .. } => "critical",
            Alert::NodeHighCpu { cpu_pct, .. } | Alert::GuestHighCpu { cpu_pct, .. }
                if *cpu_pct > 95.0 => "critical",
            Alert::NodeHighDisk { disk_pct, .. } | Alert::DiskFull { use_pct: disk_pct, .. }
                if *disk_pct > 95.0 => "critical",
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
            Alert::NodeHighCpu { node, cpu_pct } =>
                format!("Node {node} CPU at {cpu_pct:.1}%"),
            Alert::NodeHighMemory { node, mem_pct } =>
                format!("Node {node} memory at {mem_pct:.1}%"),
            Alert::NodeHighDisk { node, disk_pct } =>
                format!("Node {node} disk at {disk_pct:.1}%"),
            Alert::GuestDown { name, vmid, node } =>
                format!("Guest {name} (vmid {vmid}) down on {node}"),
            Alert::GuestHighCpu { name, vmid, cpu_pct } =>
                format!("Guest {name} ({vmid}) CPU at {cpu_pct:.1}%"),
            Alert::GuestHighMemory { name, vmid, mem_pct } =>
                format!("Guest {name} ({vmid}) memory at {mem_pct:.1}%"),
            Alert::DiskFull { name, mountpoint, use_pct, .. } =>
                format!("Disk {mountpoint} on {name} at {use_pct:.1}%"),
            Alert::LogPattern(l) =>
                format!("[{}] {} matched '{}': {}",
                    l.severity.as_str(), l.source, l.pattern,
                    &l.line[..l.line.len().min(100)]),
            Alert::StorageUnavailable { storage, node } =>
                format!("Storage {storage} unavailable on {node}"),
            Alert::HaproxyBackendDown { proxy, server, duration_secs } =>
                format!("HAProxy {proxy}/{server} DOWN for {duration_secs}s"),
        }
    }
}



// ──────────────────────────────────────────────────────────────────────────────
// Webhook payload (Alertmanager-compatible)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct WebhookPayload {
    alerts: Vec<WebhookAlert>,
}

#[derive(Serialize)]
struct WebhookAlert {
    status: &'static str,
    labels: HashMap<String, String>,
    annotations: HashMap<String, String>,
    #[serde(rename = "generatorURL")]
    generator_url: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Alert dispatcher
// ──────────────────────────────────────────────────────────────────────────────

pub struct AlertDispatcher {
    cfg: AlertConfig,
    http: Option<Client>,
    silence_map: HashMap<String, Instant>,
    storage: Option<Arc<Storage>>,
}

impl AlertDispatcher {
    pub fn new(cfg: AlertConfig, storage: Option<Arc<Storage>>) -> Self {
        let http = if cfg.enabled {
            cfg.webhook_url.as_ref().map(|_| {
                Client::builder()
                    .timeout(Duration::from_secs(10))
                    .build()
                    .expect("HTTP client")
            })
        } else {
            None
        };
        Self {
            cfg,
            http,
            silence_map: HashMap::new(),
            storage,
        }
    }

    pub async fn dispatch(&mut self, alert: Alert) {
        let key = alert.key();
        let summary = alert.summary();
        let severity = alert.severity();

        // Deduplication
        if let Some(last) = self.silence_map.get(&key) {
            if last.elapsed() < SILENCE_WINDOW {
                return;
            }
        }
        self.silence_map.insert(key.clone(), Instant::now());

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

        // Webhook dispatch
        if self.cfg.enabled {
            if let Some(ref url) = self.cfg.webhook_url {
                if let Some(ref client) = self.http {
                    let mut labels = HashMap::new();
                    labels.insert("alertname".into(), key);
                    labels.insert("severity".into(), severity.to_string());

                    let mut annotations = HashMap::new();
                    annotations.insert("summary".into(), summary);

                    let payload = WebhookPayload {
                        alerts: vec![WebhookAlert {
                            status: "firing",
                            labels,
                            annotations,
                            generator_url: String::new(),
                        }],
                    };

                    if let Err(e) = client.post(url).json(&payload).send().await {
                        error!("Webhook failed: {e}");
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
            alerts.push(Alert::NodeHighCpu { node: n.node.clone(), cpu_pct });
        }

        if n.mem_total > 0 {
            let mem_pct = (n.mem_used as f64 / n.mem_total as f64) * 100.0;
            if mem_pct > self.cfg.memory_threshold {
                alerts.push(Alert::NodeHighMemory { node: n.node.clone(), mem_pct });
            }
        }

        if n.disk_total > 0 {
            let disk_pct = (n.disk_used as f64 / n.disk_total as f64) * 100.0;
            if disk_pct > self.cfg.disk_threshold {
                alerts.push(Alert::NodeHighDisk { node: n.node.clone(), disk_pct });
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
