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
mod cluster;
mod collectors;
mod config;
mod exporter;
mod proxmox_api;
mod storage;

use alerts::{Alert, AlertDispatcher};
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
            dispatcher
                .dispatch(Alert::LogPattern(log_alert))
                .await;
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

    // Metrics HTTP server (serves Prometheus, WebSockets, and optionally /api/v1/ingest)
    let metrics_server = prom::MetricsServer::new(
        &cfg.metrics.listen_addr,
        cfg.metrics.listen_port,
        ws_tx.clone(),
        hub_state,
        cfg.metrics.auth.clone(),
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

            loop {
                ticker.tick().await;

                let mut ws_nodes = Vec::new();
                let mut ws_guests = Vec::new();

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
        let storage = storage.clone();

        tokio::spawn(async move {
            let mut ticker = interval(Duration::from_secs(cgroup_secs));
            let mut watched_lxcs: std::collections::HashSet<u32> = std::collections::HashSet::new();
            let mut dispatcher = AlertDispatcher::new(cfg_inner.alerts.clone(), Some(storage.clone()));

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
