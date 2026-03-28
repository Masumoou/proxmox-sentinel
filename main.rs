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
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, broadcast};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;
use serde_json::json;

mod alerts;
mod collectors {
    pub mod lxc;
    pub mod logs;
    pub mod vm;
}
mod config;
mod exporter {
    pub mod prometheus;
}
mod proxmox_api;

use alerts::{Alert, AlertDispatcher};
use collectors::lxc::LxcCollector;
use collectors::logs::{LogCollector, CONTAINER_LOGS, PROXMOX_HOST_LOGS};
use collectors::vm::VmCollector;
use config::Config;
use exporter::prometheus as prom;
use proxmox_api::{GuestKind, ProxmoxClient};

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

// ──────────────────────────────────────────────────────────────────────────────
// Main loop
// ──────────────────────────────────────────────────────────────────────────────

async fn run(cfg: Config) -> Result<()> {
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

    // Alert dispatcher task
    let alert_cfg = cfg.alerts.clone();
    tokio::spawn(async move {
        let mut dispatcher = AlertDispatcher::new(alert_cfg);
        while let Some(log_alert) = alert_rx.recv().await {
            dispatcher
                .dispatch(Alert::LogPattern(log_alert))
                .await;
        }
    });

    // Metrics HTTP server
    let metrics_server = prom::MetricsServer::new(
        &cfg.metrics.listen_addr,
        cfg.metrics.listen_port,
        ws_tx.clone(),
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
    let svc_secs = cfg.collection.service_check_interval_secs;

    let nodes = Arc::new(nodes);
    let cfg = Arc::new(cfg);

    // ── Task 1: Proxmox API poll (node + guest status) ─────────────────────
    {
        let client = client.clone();
        let nodes = nodes.clone();
        let cfg = cfg.clone();
        let ws_tx = ws_tx.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(api_secs));
            let mut dispatcher = AlertDispatcher::new(cfg.alerts.clone());

            loop {
                ticker.tick().await;

                let mut ws_nodes = Vec::new();
                let mut ws_guests = Vec::new();

                for node in nodes.iter() {
                    // Node status
                    match client.node_status(node).await {
                        Ok(status) => {
                            prom::update_node(&status);
                            ws_nodes.push(json!({
                                "node": status.node,
                                "cpu": status.cpu_usage,
                                "mem_used": status.mem_used,
                                "mem_total": status.mem_total,
                                "status": "online"
                            }));

                            for a in dispatcher.check_node(&status) {
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
                                ws_guests.push(json!({
                                    "vmid": guest.vmid,
                                    "name": guest.name,
                                    "node": guest.node,
                                    "type": match guest.kind { GuestKind::Vm => "qemu", GuestKind::Lxc => "lxc" },
                                    "status": guest.status,
                                    "cpu": guest.cpu_usage,
                                    "mem": guest.mem_used,
                                    "maxmem": guest.mem_total
                                }));

                                for a in dispatcher.check_guest(guest) {
                                    dispatcher.dispatch(a).await;
                                }
                            }
                        }
                        Err(e) => warn!("Guest list {node}: {e}"),
                    }

                    // Storage
                    match client.storage_status(node).await {
                        Ok(storages) => {
                            for s in &storages {
                                prom::update_storage(s);
                                if !s.active && s.enabled {
                                    dispatcher
                                        .dispatch(Alert::StorageUnavailable {
                                            storage: s.storage.clone(),
                                            node: s.node.clone(),
                                        })
                                        .await;
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
                    "guests": ws_guests
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

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(cgroup_secs));
            let mut watched_lxcs: std::collections::HashSet<u32> = std::collections::HashSet::new();
            let mut dispatcher = AlertDispatcher::new(cfg_inner.alerts.clone());

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

                        // Build services JSON
                        let svcs: Vec<serde_json::Value> = stats.services.iter().map(|s| {
                            json!({
                                "name": s.name,
                                "status": if s.state == "active" { "running" } else { "stopped" }
                            })
                        }).collect();

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

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(vm_secs));

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

                        let svcs: Vec<serde_json::Value> = vm_stats.services.iter().map(|s| {
                            json!({
                                "name": s.name,
                                "status": if s.active { "running" } else { "stopped" }
                            })
                        }).collect();

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

    // ── Block forever ─────────────────────────────────────────────────────
    info!("All collectors running. Ctrl-C to stop.");
    tokio::signal::ctrl_c().await?;
    info!("Shutting down.");
    Ok(())
}
