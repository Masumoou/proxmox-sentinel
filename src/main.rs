// src/main.rs
//
// proxmox-sentinel — lightweight agentless Proxmox monitoring daemon
//
// Architecture:
//   ┌─────────────────────────────────────────────────────────┐
//   │                   Proxmox Host Node                     │
//   │                                                         │
//   │  proxmox-sentinel (this binary, ~5-15MB RSS)            │
//   │  ├── API poller      → Proxmox REST API (nodes/VMs)     │
//   │  ├── cgroup reader   → /sys/fs/cgroup/lxc/<id>/         │
//   │  ├── inotify watcher → /var/lib/lxc/<id>/rootfs/log/   │
//   │  ├── nsenter/pct     → Service status inside LXCs       │
//   │  ├── SSH collector   → KVM VM logs + services           │
//   │  └── metrics server  → :9101/metrics (Prometheus)       │
//   └─────────────────────────────────────────────────────────┘

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use serde_json::json;

mod alerts;
mod alert_rules;
mod cluster;
mod collectors;
mod config;
mod exporter;
mod intelligence;
mod proxmox_api;
mod storage;

use alerts::{Alert, AlertDispatcher};
use alert_rules::AlertRuleEvaluator;
use collectors::lxc::LxcCollector;
use collectors::logs::{LogCollector, CONTAINER_LOGS, PROXMOX_HOST_LOGS};
use collectors::vm::VmCollector;
use collectors::haproxy::HaproxyCollector;
use config::Config;
use exporter::prometheus as prom;
use proxmox_api::{GuestKind, ProxmoxClient};
use storage::Storage;

// ──────────────────────────────────────────────────────────────────────────────
// CLI
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(name = "proxmox-sentinel", version, about = "Agentless Proxmox monitoring daemon")]
struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/proxmox-sentinel/config.toml")]
    config: PathBuf,

    #[command(subcommand)]
    cmd: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive first-run setup. Writes config.toml and systemd service.
    Init {
        /// Overwrite existing config/service files
        #[arg(long)]
        force: bool,
    },
    /// Check API, host permissions, port binding, config, and systemd install.
    Doctor,
    /// Print example configuration to stdout
    PrintConfig,
    /// Test Proxmox API connectivity and print node list
    TestApi,
}

// ──────────────────────────────────────────────────────────────────────────────
// Main
// ──────────────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    // Logging: RUST_LOG=info proxmox-sentinel
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse();

    match cli.cmd {
        Some(Commands::Init { force }) => {
            run_init(&cli.config, force)?;
            return Ok(());
        }
        Some(Commands::Doctor) => {
            run_doctor(&cli.config).await?;
            return Ok(());
        }
        Some(Commands::PrintConfig) => {
            Config::write_example();
            return Ok(());
        }
        Some(Commands::TestApi) => {
            let cfg = Config::load(&cli.config)?;
            let client = ProxmoxClient::new(&cfg.proxmox)?;
            let nodes = client.list_nodes().await?;
            println!("Connected! Nodes: {:?}", nodes);
            for node in &nodes {
                let status = client.node_status(node).await?;
                println!(
                    "  {}: CPU={:.1}% MEM={:.1}% PVE={}",
                    node,
                    status.cpu_usage * 100.0,
                    (status.mem_used as f64 / status.mem_total as f64) * 100.0,
                    status.pve_version
                );
            }
            return Ok(());
        }
        None => {}
    }

    let cfg = Config::load(&cli.config)?;
    info!("proxmox-sentinel starting");

    run(cfg).await
}

fn run_init(config_path: &Path, force: bool) -> Result<()> {
    let api_url = prompt("Proxmox API URL", "https://127.0.0.1:8006")?;
    let token_id = prompt("API token ID", "sentinel@pve!monitoring")?;
    let token_secret = prompt_secret("API token secret")?;
    let listen_port = prompt("Listen port", "9101")?.parse::<u16>().context("listen port must be a number")?;
    let verify_tls = prompt_bool("Verify TLS certificates?", false)?;
    let dashboard_auth = prompt_bool("Enable dashboard auth?", false)?;
    let prometheus = prompt_bool("Enable Prometheus endpoint?", true)?;

    if config_path.exists() && !force {
        anyhow::bail!("{} already exists. Re-run with --force to overwrite.", config_path.display());
    }

    let auth_line = if dashboard_auth {
        let user = prompt("Dashboard username", "admin")?;
        let pass = prompt_secret("Dashboard password")?;
        format!("auth = \"{}:{}\"", user, pass)
    } else {
        "auth = \"\"".to_string()
    };

    let cfg = format!(
        r#"[proxmox]
api_url = "{api_url}"
api_token_id = "{token_id}"
api_token_secret = "{token_secret}"
nodes = []
insecure_tls = {insecure_tls}

[metrics]
listen_addr = "0.0.0.0"
listen_port = {listen_port}
{auth_line}
prometheus_enabled = {prometheus}

[logs]
tail_lines = 100
buffer_size = 10000
watch_paths = []

[alerts]
enabled = true
webhook_url = ""
cpu_threshold = 90.0
memory_threshold = 85.0
disk_threshold = 90.0

[ssh]
private_key_path = "/root/.ssh/id_ed25519"
user = "root"
timeout_secs = 10
skip_vmids = []

[collection]
api_interval_secs = 15
cgroup_interval_secs = 5
vm_interval_secs = 30
service_check_interval_secs = 60

[services]
auto_discover = true
alert_on_discovered = true

[haproxy]
enabled = false
stats_url = "http://127.0.0.1:8404/stats;csv"
interval_secs = 10

[storage]
db_path = "/var/lib/proxmox-sentinel/sentinel.db"
metric_retention_days = 7
log_retention_days = 14
alert_retention_days = 30

[cluster]
mode = "standalone"
server_url = "http://127.0.0.1:{listen_port}"
shared_secret = "change_me"

[platform]
enabled = true
interval_secs = 60
backup_warn_hours = 48
backup_critical_hours = 72
task_long_running_minutes = 60
snapshot_warn_days = 7
snapshot_max_count = 5
zfs_usage_threshold = 80.0
lvmthin_data_warn_pct = 85.0
lvmthin_data_critical_pct = 95.0
lvmthin_metadata_warn_pct = 75.0
lvmthin_metadata_critical_pct = 90.0
security_enabled = true
exclude_backup_vmids = []
exclude_guest_agent_vmids = []
exclude_snapshot_vmids = []
ignore_templates = true
ignore_stopped_guests_for_backup = true

[backup_policy]
enabled = true
default_required = true
ignore_stopped_guests = true
ignore_templates = true
warn_hours = 48
critical_hours = 72
exclude_vmids = []
include_tags = []
exclude_tags = ["nobackup", "test", "template"]

[[backup_policy.tag_rules]]
tag = "critical"
warn_hours = 24
critical_hours = 36
required = true

[[backup_policy.tag_rules]]
tag = "daily-backup"
warn_hours = 36
critical_hours = 48
required = true

[certificates]
warn_days = 30
critical_days = 7
"#,
        insecure_tls = !verify_tls,
    );

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(config_path, cfg).with_context(|| format!("writing {}", config_path.display()))?;

    let service_path = Path::new("/etc/systemd/system/proxmox-sentinel.service");
    if service_path.exists() && !force {
        println!("systemd service already exists: {}", service_path.display());
    } else {
        let service = format!(
            r#"[Unit]
Description=Proxmox Sentinel
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/proxmox-sentinel --config {}
Restart=always
RestartSec=5
User=root
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
"#,
            config_path.display()
        );
        std::fs::write(service_path, service).with_context(|| format!("writing {}", service_path.display()))?;
    }

    println!("Wrote {}", config_path.display());
    println!("Wrote {}", service_path.display());
    println!("Next: systemctl daemon-reload && systemctl enable --now proxmox-sentinel");
    Ok(())
}

async fn run_doctor(config_path: &Path) -> Result<()> {
    let mut failures = 0usize;
    let mut check = |name: &str, result: Result<String>| {
        match result {
            Ok(detail) => println!("OK   {name}: {detail}"),
            Err(e) => {
                failures += 1;
                println!("FAIL {name}: {e}");
            }
        }
    };

    check("config file", Config::load(config_path).and_then(|cfg| {
        cfg.validate()?;
        Ok(format!("valid ({})", config_path.display()))
    }));

    let cfg = Config::load(config_path)?;
    let client = ProxmoxClient::new(&cfg.proxmox)?;

    check("Proxmox API", async {
        client.list_nodes().await.map(|nodes| format!("connected, {} nodes", nodes.len()))
    }.await);

    let nodes = client.list_nodes().await.unwrap_or_default();
    check("list nodes", if nodes.is_empty() { anyhow::bail!("no nodes returned") } else { Ok(nodes.join(", ")) });

    let mut guest_count = 0usize;
    for node in &nodes {
        guest_count += client.list_guests(node).await.unwrap_or_default().len();
    }
    check("list guests", if guest_count == 0 { anyhow::bail!("no guests returned") } else { Ok(format!("{guest_count} guests")) });

    check("cgroup access", std::fs::read_dir("/sys/fs/cgroup")
        .map(|_| "/sys/fs/cgroup readable".to_string())
        .map_err(Into::into));

    check("LXC rootfs logs", std::fs::read_dir("/var/lib/lxc")
        .map(|_| "/var/lib/lxc readable".to_string())
        .map_err(Into::into));

    check("bind port", check_port_or_running_sentinel(&cfg).await);

    check("systemd service", if Path::new("/etc/systemd/system/proxmox-sentinel.service").exists() {
        Ok("installed".to_string())
    } else {
        anyhow::bail!("missing /etc/systemd/system/proxmox-sentinel.service")
    });

    if failures > 0 {
        anyhow::bail!("{failures} doctor checks failed");
    }
    Ok(())
}

async fn check_port_or_running_sentinel(cfg: &Config) -> Result<String> {
    match TcpListener::bind((cfg.metrics.listen_addr.as_str(), cfg.metrics.listen_port)) {
        Ok(_) => Ok(format!("{}:{} available", cfg.metrics.listen_addr, cfg.metrics.listen_port)),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            let health_host = if cfg.metrics.listen_addr == "0.0.0.0" || cfg.metrics.listen_addr == "::" {
                "127.0.0.1"
            } else {
                cfg.metrics.listen_addr.as_str()
            };
            let url = format!("http://{health_host}:{}/health", cfg.metrics.listen_port);
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .context("building doctor health client")?;
            let mut request = client.get(&url);
            if let Some(auth) = cfg.metrics.auth.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                if let Some((user, pass)) = auth.split_once(':') {
                    request = request.basic_auth(user.to_string(), Some(pass.to_string()));
                }
            }
            let response = request.send().await.with_context(|| format!("checking {url}"))?;
            if response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                if body.trim() == "OK" {
                    return Ok(format!(
                        "{}:{} already in use by running Sentinel (/health OK)",
                        cfg.metrics.listen_addr, cfg.metrics.listen_port
                    ));
                }
            }
            anyhow::bail!("{}:{} is already in use, but Sentinel /health did not return OK", cfg.metrics.listen_addr, cfg.metrics.listen_port)
        }
        Err(e) => Err(e).with_context(|| format!("binding {}:{}", cfg.metrics.listen_addr, cfg.metrics.listen_port)),
    }
}

fn prompt(label: &str, default: &str) -> Result<String> {
    print!("{label} [{default}]: ");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim();
    Ok(if value.is_empty() { default.to_string() } else { value.to_string() })
}

fn prompt_secret(label: &str) -> Result<String> {
    print!("{label}: ");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    let value = value.trim();
    if value.is_empty() {
        anyhow::bail!("{label} is required");
    }
    Ok(value.to_string())
}

fn prompt_bool(label: &str, default: bool) -> Result<bool> {
    let default_label = if default { "Y/n" } else { "y/N" };
    print!("{label} [{default_label}]: ");
    io::stdout().flush()?;
    let mut value = String::new();
    io::stdin().read_line(&mut value)?;
    match value.trim().to_lowercase().as_str() {
        "" => Ok(default),
        "y" | "yes" | "true" | "1" => Ok(true),
        "n" | "no" | "false" | "0" => Ok(false),
        _ => anyhow::bail!("answer yes or no"),
    }
}

fn normalize_service_name(name: &str) -> String {
    name.strip_suffix(".service").unwrap_or(name).to_string()
}

fn service_is_healthy(state: &str, sub_state: &str) -> bool {
    matches!(state, "active" | "started") && !matches!(sub_state, "failed" | "dead")
}

fn is_public_bind_without_auth(cfg: &Config) -> bool {
    let auth_empty = cfg.metrics.auth.as_deref().map(str::trim).unwrap_or("").is_empty();
    let public_bind = matches!(
        cfg.metrics.listen_addr.as_str(),
        "0.0.0.0" | "::" | "[::]" | ""
    );
    auth_empty && public_bind
}

// ──────────────────────────────────────────────────────────────────────────────
// Main loop
// ──────────────────────────────────────────────────────────────────────────────

async fn run(cfg: Config) -> Result<()> {
    if is_public_bind_without_auth(&cfg) {
        warn!(
            "WARNING: Sentinel is listening on {}:{} without dashboard auth. Do not expose this endpoint to untrusted networks.",
            cfg.metrics.listen_addr,
            cfg.metrics.listen_port
        );
    }

    let client = Arc::new(ProxmoxClient::new(&cfg.proxmox)?);

    // WebSocket broadcast channel (created first so LogCollector can use it)
    let (ws_tx, _) = broadcast::channel::<String>(128);

    // Alert channel: log watcher → dispatcher
    let (alert_tx, mut alert_rx) = mpsc::unbounded_channel();

    // Log collector (shared) — receives ws_tx for live log streaming
    let log_collector = Arc::new(LogCollector::new(
        cfg.logs.clone(),
        alert_tx.clone(),
        Some(ws_tx.clone()),
    ));

    // Watch Proxmox host logs
    for log_path in PROXMOX_HOST_LOGS {
        log_collector.watch_host_log(log_path).await.ok();
    }

    // Check and create storage dir
    let storage_path = PathBuf::from(&cfg.storage.db_path);
    let storage = Arc::new(Storage::open(&storage_path)?);

    // Retention cleanup task
    {
        let store = storage.clone();
        let s_cfg = cfg.storage.clone();
        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(3600)); // Every hour
            loop {
                ticker.tick().await;
                match store.cleanup_old_data(
                    s_cfg.metric_retention_days,
                    s_cfg.log_retention_days,
                    s_cfg.alert_retention_days,
                ) {
                    Ok(stats) => info!("{}", stats),
                    Err(e) => warn!("Storage cleanup error: {}", e),
                }
            }
        });
    }

    // Alert dispatcher task
    let alert_cfg = cfg.alerts.clone();
    let alert_store = storage.clone();
    tokio::spawn(async move {
        let mut dispatcher = AlertDispatcher::new(alert_cfg, Some(alert_store));
        while let Some(log_alert) = alert_rx.recv().await {
            let lower_line = log_alert.line.to_lowercase();
            if lower_line.contains("out of memory: killed process") {
                let process = log_alert.line.split("process")
                    .nth(1)
                    .unwrap_or("")
                    .split_whitespace()
                    .next()
                    .unwrap_or("unknown")
                    .to_string();
                crate::exporter::prometheus::inc_oom_killer(&log_alert.source);
                dispatcher.dispatch(Alert::OomKilled { node: log_alert.source.clone(), process }).await;
            } else {
                dispatcher
                    .dispatch(Alert::LogPattern(log_alert))
                    .await;
            }
        }
    });

    // If agent mode, spawn the forwarding task
    if cfg.cluster.mode == "agent" {
        let agent_rx = ws_tx.subscribe();
        let agent_cfg = Arc::new(cfg.clone());
        tokio::spawn(async move {
            cluster::run_agent(agent_cfg, agent_rx).await;
        });
    }

    // If server mode, we need HubState for the API route
    let hub_state = if cfg.cluster.mode == "server" {
        Some(cluster::HubState {
            ws_tx: ws_tx.clone(),
            storage: storage.clone(),
            secret: cfg.cluster.shared_secret.clone(),
        })
    } else {
        None
    };

    let metrics_server = prom::MetricsServer::new(
        &cfg.metrics.listen_addr,
        cfg.metrics.listen_port,
        ws_tx.clone(),
        Some(storage.clone()),
        hub_state,
        cfg.metrics.auth.clone(),
        cfg.metrics.prometheus_enabled,
    );
    tokio::spawn(async move {
        if let Err(e) = metrics_server.run().await {
            error!("Metrics server error: {e}");
        }
    });

    // Determine nodes to monitor
    let nodes = if cfg.proxmox.nodes.is_empty() {
        client.list_nodes().await.context("Listing nodes")?
    } else {
        cfg.proxmox.nodes.clone()
    };

    info!("Monitoring nodes: {:?}", nodes);

    // ── Polling intervals ─────────────────────────────────────────────────

    let api_secs = cfg.collection.api_interval_secs;
    let cgroup_secs = cfg.collection.cgroup_interval_secs;
    let vm_secs = cfg.collection.vm_interval_secs;
    let _svc_secs = cfg.collection.service_check_interval_secs;

    let nodes = Arc::new(nodes);
    let cfg = Arc::new(cfg);

    // ── Task 1: Proxmox API poll (node + guest status) ─────────────────────
    {
        let client = client.clone();
        let nodes = nodes.clone();
        let cfg = cfg.clone();
        let ws_tx = ws_tx.clone();
        let storage = storage.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(api_secs));
            let mut dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
            let mut rule_evaluator = AlertRuleEvaluator::new();
            let mut vm_last_node: std::collections::HashMap<u32, String> = std::collections::HashMap::new();

            loop {
                ticker.tick().await;

                let mut ws_nodes = Vec::new();
                let mut ws_guests = Vec::new();
                let mut ws_storage = Vec::new();

                for node in nodes.iter() {
                    // Node status
                    match client.node_status(node).await {
                        Ok(status) => {
                            prom::update_node(&status);

                            if let Err(e) = storage.insert_node_metric(
                                &status.node, status.cpu_usage, status.mem_used, status.mem_total,
                                status.swap_used, status.swap_total, status.disk_used, status.disk_total,
                                status.load_avg1,
                            ) {
                                warn!("SQLite node metric error: {}", e);
                            }

                            ws_nodes.push(json!({
                                "node": status.node,
                                "cpu": status.cpu_usage,
                                "mem_used": status.mem_used,
                                "mem_total": status.mem_total,
                                "swap_used": status.swap_used,
                                "swap_total": status.swap_total,
                                "disk_used": status.disk_used,
                                "disk_total": status.disk_total,
                                "status": "online"
                            }));

                            for a in dispatcher.check_node(&status) {
                                dispatcher.dispatch(a).await;
                            }
                            for a in rule_evaluator.evaluate_node(&cfg.alert_rules, &status) {
                                dispatcher.dispatch(a).await;
                            }
                        }
                        Err(e) => warn!("Node status {node}: {e}"),
                    }

                    // Guest list
                    match client.list_guests(node).await {
                        Ok(guests) => {
                            for guest in &guests {
                                prom::update_guest(guest);

                                if let Err(e) = storage.insert_guest_metric(
                                    guest.vmid, &guest.name, match guest.kind { GuestKind::Vm => "qemu", GuestKind::Lxc => "lxc" },
                                    &guest.status, guest.cpu_usage, guest.mem_used, guest.mem_total, node
                                ) {
                                    warn!("SQLite guest metric error: {}", e);
                                }

                                ws_guests.push(json!({
                                    "vmid": guest.vmid,
                                    "name": guest.name,
                                    "node": guest.node,
                                    "type": match guest.kind { GuestKind::Vm => "qemu", GuestKind::Lxc => "lxc" },
                                    "status": guest.status,
                                    "cpu": guest.cpu_usage,
                                    "maxcpu": guest.cpu_count,
                                    "mem": guest.mem_used,
                                    "maxmem": guest.mem_total,
                                    "os_name": guest.os_name.clone(),
                                    "os_version": guest.os_version.clone(),
                                    "tags": guest.tags.clone(),
                                    "template": guest.template
                                }));

                                for a in dispatcher.check_guest(guest) {
                                    dispatcher.dispatch(a).await;
                                }
                                for a in rule_evaluator.evaluate_guest(&cfg.alert_rules, guest) {
                                    dispatcher.dispatch(a).await;
                                }

                                if let Some(old) = vm_last_node.get(&guest.vmid) {
                                    if old != &guest.node {
                                        dispatcher.dispatch(Alert::MigrationDetected {
                                            vmid: guest.vmid,
                                            name: guest.name.clone(),
                                            from_node: old.clone(),
                                            to_node: guest.node.clone(),
                                        }).await;
                                        
                                        let _ = ws_tx.send(json!({
                                            "type": "vm_migrated",
                                            "vmid": guest.vmid,
                                            "name": guest.name,
                                            "from": old,
                                            "to": guest.node,
                                            "timestamp": chrono::Utc::now().to_rfc3339()
                                        }).to_string());
                                    }
                                }
                                vm_last_node.insert(guest.vmid, guest.node.clone());
                            }
                        }
                        Err(e) => warn!("Guest list {node}: {e}"),
                    }

                    // Storage
                    match client.storage_status(node).await {
                        Ok(storages) => {
                            for s in &storages {
                                prom::update_storage(s);
                                ws_storage.push(json!({
                                    "storage": s.storage.clone(),
                                    "node": s.node.clone(),
                                    "type": s.kind.clone(),
                                    "content": s.content.clone(),
                                    "used": s.used,
                                    "total": s.total,
                                    "avail": s.avail,
                                    "active": s.active,
                                    "enabled": s.enabled
                                }));
                                if !s.active && s.enabled {
                                    dispatcher
                                        .dispatch(Alert::StorageUnavailable {
                                            storage: s.storage.clone(),
                                            node: s.node.clone(),
                                        })
                                        .await;
                                }
                                for a in rule_evaluator.evaluate_storage(&cfg.alert_rules, s) {
                                    dispatcher.dispatch(a).await;
                                }
                            }
                        }
                        Err(e) => warn!("Storage status {node}: {e}"),
                    }
                }

                // Broadcast live state to WebSocket clients
                let event = json!({
                    "type": "cluster_update",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "nodes": ws_nodes,
                    "guests": ws_guests,
                    "storage": ws_storage
                });
                let _ = ws_tx.send(event.to_string());
            }
        });
    }

    // ── Task 2: cgroup stats for running LXCs ─────────────────────────────
    {
        let client = client.clone();
        let nodes = nodes.clone();
        let cfg_inner = cfg.clone();
        let log_collector = log_collector.clone();
        let ws_tx = ws_tx.clone();
        let storage = storage.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(cgroup_secs));
            let mut watched_lxcs: std::collections::HashSet<u32> = std::collections::HashSet::new();
            let mut discovered_lxc_services: std::collections::HashMap<u32, std::collections::HashSet<String>> = std::collections::HashMap::new();
            let mut dispatcher = AlertDispatcher::new(cfg_inner.alerts.clone(), Some(storage.clone()));
            let mut rule_evaluator = AlertRuleEvaluator::new();

            loop {
                ticker.tick().await;

                let mut lxc_details = Vec::new();

                for node in nodes.iter() {
                    let guests = match client.list_guests(node).await {
                        Ok(g) => g,
                        Err(_) => continue,
                    };

                    for guest in guests
                        .iter()
                        .filter(|g| g.kind == GuestKind::Lxc && g.status == "running")
                    {
                        let stats = LxcCollector::collect(guest.vmid, &guest.name).await;
                        prom::update_lxc_detail(&stats);

                        let tracked_services = cfg_inner.services.lxc.iter().find(|l| l.vmid == guest.vmid);

                        let active_services: std::collections::HashSet<String> = stats.services
                            .iter()
                            .filter(|s| service_is_healthy(&s.state, &s.sub_state))
                            .map(|s| normalize_service_name(&s.name))
                            .collect();

                        let should_show_service = |name: &str| {
                            if cfg_inner.services.auto_discover {
                                return true;
                            }
                            tracked_services
                                .map(|tracked| {
                                    let short = normalize_service_name(name);
                                    tracked.checks.contains(&name.to_string()) || tracked.checks.contains(&short)
                                })
                                .unwrap_or(false)
                        };

                        let svcs: Vec<serde_json::Value> = stats.services.iter()
                            .filter(|s| should_show_service(&s.name))
                            .map(|s| {
                                let is_active = service_is_healthy(&s.state, &s.sub_state);
                                json!({
                                    "name": normalize_service_name(&s.name),
                                    "status": if is_active { "running" } else { "failed" },
                                    "state": s.state.as_str(),
                                    "sub_state": s.sub_state.as_str()
                                })
                            })
                            .collect();
                        
                        if !stats.services.is_empty() {
                            // Dispatch alerts for explicitly tracked services that are down or missing.
                            if let Some(tracked) = tracked_services {
                                for service in &tracked.checks {
                                    let name = normalize_service_name(service);
                                    if !active_services.contains(&name) {
                                        dispatcher.dispatch(Alert::ServiceUnavailable {
                                            vmid: guest.vmid,
                                            node: guest.node.clone(),
                                            service: name,
                                        }).await;
                                    }
                                }
                            }

                            if cfg_inner.services.alert_on_discovered {
                                let baseline = discovered_lxc_services.entry(guest.vmid).or_default();
                                if baseline.is_empty() {
                                    baseline.extend(active_services.iter().cloned());
                                } else {
                                    let missing: Vec<String> = baseline
                                        .difference(&active_services)
                                        .cloned()
                                        .collect();
                                    for service in missing {
                                        dispatcher.dispatch(Alert::ServiceUnavailable {
                                            vmid: guest.vmid,
                                            node: guest.node.clone(),
                                            service,
                                        }).await;
                                    }
                                    baseline.extend(active_services.iter().cloned());
                                }
                            }
                            for alert in rule_evaluator.evaluate_services(
                                &cfg_inner.alert_rules,
                                guest.vmid,
                                &guest.node,
                                &active_services,
                            ) {
                                dispatcher.dispatch(alert).await;
                            }
                        }

                        // Build disk mounts JSON
                        let disks: Vec<serde_json::Value> = stats.disk_mounts.iter().map(|d| {
                            json!({
                                "mountpoint": d.mountpoint,
                                "total": d.total,
                                "used": d.used,
                                "use_pct": d.use_pct
                            })
                        }).collect();

                        lxc_details.push(json!({
                            "vmid": guest.vmid,
                            "name": guest.name,
                            "ip": stats.ip_address.clone(),
                            "os_name": stats.os_name.clone(),
                            "os_version": stats.os_version.clone(),
                            "services": svcs,
                            "disk_mounts": disks,
                            "mem_current": stats.cgroup.mem_current,
                            "mem_limit": stats.cgroup.mem_limit,
                            "pids": stats.cgroup.pid_current
                        }));

                        for mount in &stats.disk_mounts {
                            if let Some(alert) = alerts::check_disk_threshold(
                                guest.vmid,
                                &guest.name,
                                &mount.mountpoint,
                                mount.use_pct,
                                cfg_inner.alerts.disk_threshold,
                            ) {
                                dispatcher.dispatch(alert).await;
                            }
                        }

                        if !watched_lxcs.contains(&guest.vmid) {
                            info!("Registering log watchers for LXC {}", guest.vmid);
                            for log_path in CONTAINER_LOGS {
                                log_collector
                                    .watch_lxc_log(guest.vmid, log_path)
                                    .await
                                    .ok();
                            }
                            for log_path in &cfg_inner.logs.watch_paths {
                                log_collector
                                    .watch_lxc_log(guest.vmid, log_path)
                                    .await
                                    .ok();
                            }
                            watched_lxcs.insert(guest.vmid);
                        }
                    }
                }

                // Broadcast LXC detail update
                let event = json!({
                    "type": "lxc_detail",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "lxc": lxc_details
                });
                let _ = ws_tx.send(event.to_string());
            }
        });
    }

    // ── Task 3: VM deep stats via agent + SSH ─────────────────────────────
    {
        let client = client.clone();
        let nodes = nodes.clone();
        let cfg_inner = cfg.clone();
        let ws_tx = ws_tx.clone();
        let storage = storage.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(vm_secs));
            let mut conn_failures: std::collections::HashMap<u32, u8> = std::collections::HashMap::new();
            let mut discovered_vm_services: std::collections::HashMap<u32, std::collections::HashSet<String>> = std::collections::HashMap::new();
            let mut dispatcher = AlertDispatcher::new(cfg_inner.alerts.clone(), Some(storage.clone()));
            let mut rule_evaluator = AlertRuleEvaluator::new();

            loop {
                ticker.tick().await;
                let vm_collector = VmCollector::new(&client, &cfg_inner.ssh);

                let mut vm_details = Vec::new();

                for node in nodes.iter() {
                    let guests = match client.list_guests(node).await {
                        Ok(g) => g,
                        Err(_) => continue,
                    };

                    for guest in guests
                        .iter()
                        .filter(|g| g.kind == GuestKind::Vm && g.status == "running")
                    {
                        if cfg_inner.ssh.skip_vmids.contains(&guest.vmid) {
                            continue;
                        }
                        let vm_stats = vm_collector
                            .collect(node, guest.vmid, &guest.name)
                            .await;

                        if !vm_stats.agent_available && !vm_stats.ssh_available {
                            let count = conn_failures.entry(guest.vmid).or_insert(0);
                            *count += 1;
                            if *count >= 5 {
                                dispatcher.dispatch(Alert::VmConnectionLost { 
                                    vmid: guest.vmid, 
                                    name: guest.name.clone(), 
                                    node: node.clone() 
                                }).await;
                            }
                        } else {
                            conn_failures.insert(guest.vmid, 0);
                        }

                        let active_services: std::collections::HashSet<String> = vm_stats.services
                            .iter()
                            .filter(|s| s.active)
                            .map(|s| normalize_service_name(&s.name))
                            .collect();

                        let svcs: Vec<serde_json::Value> = vm_stats.services.iter().map(|s| {
                            json!({
                                "name": normalize_service_name(&s.name),
                                "status": if s.active { "running" } else { "stopped" },
                                "state": if s.active { "active" } else { "inactive" },
                                "sub_state": s.status.as_str()
                            })
                        }).collect();

                        if !vm_stats.services.is_empty() {
                            if let Some(tracked) = cfg_inner.services.vm.iter().find(|v| {
                                v.vmid == Some(guest.vmid)
                                    || vm_stats.ip_address.as_ref().is_some_and(|ip| v.ip.as_ref() == Some(ip))
                            }) {
                                    for service in &tracked.checks {
                                        let name = normalize_service_name(service);
                                        if !active_services.contains(&name) {
                                            dispatcher.dispatch(Alert::ServiceUnavailable {
                                                vmid: guest.vmid,
                                                node: node.clone(),
                                                service: name,
                                            }).await;
                                        }
                                    }
                            }

                            if cfg_inner.services.alert_on_discovered {
                                let baseline = discovered_vm_services.entry(guest.vmid).or_default();
                                if baseline.is_empty() {
                                    baseline.extend(active_services.iter().cloned());
                                } else {
                                    let missing: Vec<String> = baseline
                                        .difference(&active_services)
                                        .cloned()
                                        .collect();
                                    for service in missing {
                                        dispatcher.dispatch(Alert::ServiceUnavailable {
                                            vmid: guest.vmid,
                                            node: node.clone(),
                                            service,
                                        }).await;
                                    }
                                    baseline.extend(active_services.iter().cloned());
                                }
                            }
                            for alert in rule_evaluator.evaluate_services(
                                &cfg_inner.alert_rules,
                                guest.vmid,
                                node,
                                &active_services,
                            ) {
                                dispatcher.dispatch(alert).await;
                            }
                        }

                        let disks: Vec<serde_json::Value> = vm_stats.disk_mounts.iter().map(|d| {
                            json!({
                                "mountpoint": d.mountpoint,
                                "total": d.total,
                                "used": d.used,
                                "use_pct": d.use_pct
                            })
                        }).collect();

                        vm_details.push(json!({
                            "vmid": guest.vmid,
                            "name": guest.name,
                            "os_name": vm_stats.os_name.clone(),
                            "os_version": vm_stats.os_version.clone(),
                            "services": svcs,
                            "disk_mounts": disks,
                            "agent": vm_stats.agent_available,
                            "ssh": vm_stats.ssh_available,
                            "ip": vm_stats.ip_address
                        }));

                        info!(
                            "VM {} ({}) — agent={} ssh={} mounts={} services={}",
                            guest.name,
                            guest.vmid,
                            vm_stats.agent_available,
                            vm_stats.ssh_available,
                            vm_stats.disk_mounts.len(),
                            vm_stats.services.len(),
                        );
                    }
                }

                // Broadcast VM detail update
                let event = json!({
                    "type": "vm_detail",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "vms": vm_details
                });
                let _ = ws_tx.send(event.to_string());
            }
        });
    }

    // ── Task 4: HAProxy stats ──────────────────────────────────────────────
    if let Some(ref haproxy_cfg) = cfg.haproxy {
        if haproxy_cfg.enabled {
            let ha_cfg = haproxy_cfg.clone();
            let ws_tx = ws_tx.clone();
            let alert_cfg = cfg.alerts.clone();
            let storage = storage.clone();

            match HaproxyCollector::new(&ha_cfg) {
                Ok(collector) => {
                    info!("HAProxy monitoring enabled: {}", ha_cfg.stats_url);
                    tokio::spawn(async move {
                        let mut ticker = interval(Duration::from_secs(ha_cfg.interval_secs));
                        let mut dispatcher = AlertDispatcher::new(alert_cfg, Some(storage.clone()));

                        loop {
                            ticker.tick().await;

                            match collector.collect(&ha_cfg).await {
                                Ok(stats) => {
                                    // Update Prometheus metrics
                                    prom::update_haproxy(&stats);

                                    // Save to SQLite
                                    for p in &stats.proxies {
                                        for s in &p.servers {
                                            if let Err(e) = storage.insert_haproxy_metric(
                                                &p.name, &s.server_name, &s.status,
                                                s.sessions_current, s.bytes_in, s.bytes_out, s.http_5xx,
                                            ) {
                                                warn!("SQLite haproxy error: {}", e);
                                            }
                                        }
                                    }

                                    // Fire alerts for down servers
                                    for (proxy, server, downtime) in
                                        HaproxyCollector::find_down_servers(&stats)
                                    {
                                        dispatcher
                                            .dispatch(Alert::HaproxyBackendDown {
                                                proxy: proxy.to_string(),
                                                server: server.to_string(),
                                                duration_secs: downtime,
                                            })
                                            .await;
                                    }

                                    // Build WebSocket payload
                                    let proxies_json: Vec<serde_json::Value> = stats
                                        .proxies
                                        .iter()
                                        .map(|p| {
                                            let servers: Vec<serde_json::Value> = p
                                                .servers
                                                .iter()
                                                .map(|s| {
                                                    json!({
                                                        "name": s.server_name,
                                                        "status": s.status,
                                                        "sessions": s.sessions_current,
                                                        "bytes_in": s.bytes_in,
                                                        "bytes_out": s.bytes_out,
                                                        "http_5xx": s.http_5xx,
                                                        "check_status": s.check_status,
                                                        "downtime": s.downtime_secs,
                                                        "weight": s.weight,
                                                        "active": s.active
                                                    })
                                                })
                                                .collect();

                                            json!({
                                                "name": p.name,
                                                "frontend_status": p.frontend.as_ref().map(|f| f.status.as_str()).unwrap_or("unknown"),
                                                "backend_status": p.backend_summary.as_ref().map(|b| b.status.as_str()).unwrap_or("unknown"),
                                                "servers": servers
                                            })
                                        })
                                        .collect();

                                    let event = json!({
                                        "type": "haproxy_update",
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                        "total_servers": stats.total_servers,
                                        "servers_up": stats.servers_up,
                                        "servers_down": stats.servers_down,
                                        "proxies": proxies_json
                                    });
                                    let _ = ws_tx.send(event.to_string());
                                }
                                Err(e) => warn!("HAProxy stats error: {e}"),
                            }
                        }
                    });
                }
                Err(e) => error!("Failed to init HAProxy collector: {e}"),
            }
        }
    }

    // ── Task 5: Database and Storage Health ───────────────────────────────
    for pg_cfg in &cfg.postgres {
        if pg_cfg.enabled {
            let dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
            tokio::spawn(crate::collectors::postgres::run_collector(pg_cfg.clone(), dispatcher));
        }
    }
    for redis_cfg in &cfg.redis {
        if redis_cfg.enabled {
            let dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
            tokio::spawn(crate::collectors::redis::run_collector(redis_cfg.clone(), dispatcher));
        }
    }
    for os_cfg in &cfg.object_storage {
        if os_cfg.enabled {
            let dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
            tokio::spawn(crate::collectors::object_storage::run_collector(os_cfg.clone(), dispatcher));
        }
    }
    if cfg.file_activity.enabled {
        tokio::spawn(crate::collectors::file_activity::run_collector(cfg.file_activity.clone(), ws_tx.clone()));
    }
    
    // ── Task 6: Node Pressure Analyzer ────────────────────────────────────
    if cfg.intelligence.enabled {
        let dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
        tokio::spawn(crate::intelligence::run_analyzer(cfg.intelligence.clone(), client.clone(), dispatcher));
    }

    // ── Task 7: Application Metrics ───────────────────────────────────────
    for app_cfg in cfg.app_metrics.iter().filter(|c| c.enabled) {
        let dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
        let ws_tx_clone = ws_tx.clone();
        let storage_clone = storage.clone();
        let cfg_clone = app_cfg.clone();
        tokio::spawn(crate::collectors::app_metrics::run_collector(
            cfg_clone, storage_clone, dispatcher, ws_tx_clone
        ));
    }

    // ── Task 8: Application Logs ──────────────────────────────────────────
    for log_cfg in cfg.app_logs.iter().filter(|c| c.enabled) {
        let dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
        let ws_tx_clone = ws_tx.clone();
        let full_cfg_clone = cfg.clone();
        let cfg_clone = log_cfg.clone();
        tokio::spawn(crate::collectors::app_logs::run_collector(
            cfg_clone, full_cfg_clone, dispatcher, ws_tx_clone
        ));
    }

    // ── Task 9: Proxmox platform health ───────────────────────────────────
    if cfg.platform.enabled {
        let dispatcher = AlertDispatcher::new(cfg.alerts.clone(), Some(storage.clone()));
        tokio::spawn(crate::collectors::platform::run_collector(
            cfg.platform.clone(),
            cfg.backup_policy.clone(),
            cfg.certificates.clone(),
            client.clone(),
            nodes.clone(),
            ws_tx.clone(),
            dispatcher,
        ));
    }

    // ── Wait for shutdown signals ─────────────────────────────────────────
    info!("All collectors running. Waiting for events.");

    #[cfg(unix)]
    let mut sighup = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::hangup())?;

    #[cfg(unix)]
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Ctrl-C received, initiating graceful shutdown.");
        }
        _ = sighup.recv() => {
            info!("SIGHUP received, initiating shutdown (hot-reload is planned).");
        }
    }

    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await?;
    #[cfg(not(unix))]
    info!("Ctrl-C received, initiating graceful shutdown.");

    info!("Flushing pending webhooks and committing SQLite transactions (wait 2s)...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    info!("Shutdown complete.");
    Ok(())
}
