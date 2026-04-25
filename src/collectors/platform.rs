use crate::alerts::{Alert, AlertDispatcher};
use crate::config::{CertificateConfig, PlatformConfig};
use crate::proxmox_api::{GuestKind, ProxmoxClient};
use anyhow::Result;
use reqwest::Url;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::io::AsyncWriteExt;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{debug, warn};
use std::process::Stdio;

#[derive(Debug, Clone, Serialize)]
struct ZfsPool {
    name: String,
    state: String,
    capacity_pct: f64,
    fragmentation_pct: Option<f64>,
    scrub: String,
    errors: String,
    read_errors: u64,
    write_errors: u64,
    checksum_errors: u64,
}

#[derive(Debug, Clone, Serialize)]
struct BackupHealth {
    vmid: u32,
    name: String,
    node: String,
    kind: String,
    last_backup_ts: Option<i64>,
    age_hours: Option<i64>,
    status: String,
    task_status: String,
    size_bytes: Option<u64>,
    source: String,
}

#[derive(Debug, Clone)]
struct BackupArtifact {
    vmid: u32,
    node: String,
    storage: String,
    volid: String,
    ctime: i64,
    size_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
struct TaskHealth {
    upid: String,
    node: String,
    worker_type: String,
    vmid: Option<u32>,
    user: String,
    status: String,
    start_time: i64,
    end_time: Option<i64>,
    duration_secs: i64,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotHealth {
    vmid: u32,
    name: String,
    kind: String,
    count: usize,
    oldest_days: Option<i64>,
    snapshots: Vec<SnapshotInfo>,
}

#[derive(Debug, Clone, Serialize)]
struct SnapshotInfo {
    name: String,
    description: String,
    created_ts: Option<i64>,
    age_days: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct GuestAgentHealth {
    vmid: u32,
    name: String,
    node: String,
    status: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct SecurityCheck {
    key: String,
    label: String,
    severity: String,
    status: String,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct CertCheck {
    name: String,
    url: String,
    status: String,
    days_remaining: Option<i64>,
    expires_at: Option<String>,
    detail: String,
}

#[derive(Debug, Clone, Serialize)]
struct CephHealth {
    installed: bool,
    health: String,
    detail: String,
    osd_up: Option<u64>,
    osd_total: Option<u64>,
    mons: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ClusterHealth {
    quorum: String,
    nodes: Vec<String>,
    detail: String,
    ha_resources: Vec<Value>,
}

#[derive(Debug, Clone, Serialize)]
struct ThinPoolHealth {
    vg: String,
    lv: String,
    data_pct: f64,
    meta_pct: f64,
    status: String,
}

pub async fn run_collector(
    cfg: PlatformConfig,
    cert_cfg: CertificateConfig,
    client: Arc<ProxmoxClient>,
    nodes: Arc<Vec<String>>,
    ws_tx: broadcast::Sender<String>,
    mut dispatcher: AlertDispatcher,
) {
    if !cfg.enabled {
        return;
    }

    let mut ticker = interval(Duration::from_secs(cfg.interval_secs.max(15)));
    loop {
        ticker.tick().await;

        let mut alerts = Vec::new();
        let zfs = collect_zfs(&cfg, &mut alerts).await;
        let tasks = collect_tasks(&cfg, &mut alerts).await;
        let cluster = collect_cluster().await;
        let ceph = collect_ceph(&mut alerts).await;
        let thin_pools = collect_thin_pools(&cfg, &mut alerts).await;
        let guests = collect_all_guests(client.clone(), nodes.clone()).await;
        let guest_agents = collect_guest_agent_health(client.clone(), &guests, &mut alerts).await;
        let backup_artifacts = collect_backup_artifacts(client.clone(), nodes.clone()).await;
        let backups = collect_backups(&cfg, &guests, &tasks, &backup_artifacts, &mut alerts).await;
        let snapshots = collect_snapshots(&cfg, client.clone(), &guests, &mut alerts).await;
        let security = if cfg.security_enabled {
            collect_security(&guest_agents, &mut alerts).await
        } else {
            Vec::new()
        };
        let certificates = collect_certs(&cert_cfg, &mut alerts).await;
        update_platform_metrics(
            &zfs,
            &backups,
            &tasks,
            &cluster,
            &ceph,
            &thin_pools,
            &snapshots,
            &security,
            &certificates,
            &guest_agents,
        );

        for alert in alerts {
            dispatcher.dispatch(alert).await;
        }

        let _ = ws_tx.send(json!({
            "type": "platform_health",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "zfs": zfs,
            "backups": backups,
            "tasks": tasks,
            "cluster": cluster,
            "ceph": ceph,
            "thin_pools": thin_pools,
            "snapshots": snapshots,
            "security": security,
            "certificates": certificates,
            "guest_agents": guest_agents,
        }).to_string());
    }
}

async fn collect_all_guests(client: Arc<ProxmoxClient>, nodes: Arc<Vec<String>>) -> Vec<crate::proxmox_api::GuestStatus> {
    let mut guests = Vec::new();
    for node in nodes.iter() {
        match client.list_guests(node).await {
            Ok(mut found) => guests.append(&mut found),
            Err(e) => debug!("platform guest list {node}: {e}"),
        }
    }
    guests
}

async fn collect_zfs(cfg: &PlatformConfig, alerts: &mut Vec<Alert>) -> Vec<ZfsPool> {
    let list = run_cmd("zpool", &["list", "-H", "-o", "name,health,capacity,fragmentation"]).await;
    let status = run_cmd("zpool", &["status"]).await.unwrap_or_default();
    let Ok(list) = list else {
        return Vec::new();
    };

    let pools = parse_zfs_pools(&list, &status);
    for pool in &pools {
        if pool.state != "ONLINE" {
            alerts.push(platform_alert(
                format!("zfs_state:{}", pool.name),
                "critical",
                format!("ZFS pool {} is {}", pool.name, pool.state),
            ));
        }
        if pool.capacity_pct >= cfg.zfs_usage_threshold {
            alerts.push(platform_alert(
                format!("zfs_usage:{}", pool.name),
                if pool.capacity_pct >= 95.0 { "critical" } else { "warning" },
                format!("ZFS pool {} usage at {:.1}%", pool.name, pool.capacity_pct),
            ));
        }
        if pool.checksum_errors > 0 || pool.read_errors > 0 || pool.write_errors > 0 {
            alerts.push(platform_alert(
                format!("zfs_errors:{}", pool.name),
                "critical",
                format!(
                    "ZFS pool {} has device errors: read={} write={} checksum={}",
                    pool.name, pool.read_errors, pool.write_errors, pool.checksum_errors
                ),
            ));
        }
        if scrub_has_errors(&pool.scrub) {
            alerts.push(platform_alert(
                format!("zfs_scrub:{}", pool.name),
                "warning",
                format!("ZFS pool {} scrub reported errors: {}", pool.name, pool.scrub),
            ));
        }
    }
    pools
}

async fn collect_tasks(cfg: &PlatformConfig, alerts: &mut Vec<Alert>) -> Vec<TaskHealth> {
    let out = run_cmd("pvesh", &["get", "/cluster/tasks", "--output-format", "json"]).await.unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    let tasks = parse_tasks_json(&out, now);
    for task in &tasks {
        if task.status.to_lowercase().contains("error") || task.status.to_lowercase().contains("fail") {
            alerts.push(platform_alert(
                format!("task_failed:{}", task.upid),
                "critical",
                format!("Proxmox task {} failed on {}: {}", task.worker_type, task.node, task.status),
            ));
        }
        if task.end_time.is_none() && task.duration_secs > (cfg.task_long_running_minutes as i64 * 60) {
            alerts.push(platform_alert(
                format!("task_long:{}", task.upid),
                "warning",
                format!("Proxmox task {} on {} has been running for {} minutes", task.worker_type, task.node, task.duration_secs / 60),
            ));
        }
    }
    tasks
}

async fn collect_backups(
    cfg: &PlatformConfig,
    guests: &[crate::proxmox_api::GuestStatus],
    tasks: &[TaskHealth],
    artifacts: &[BackupArtifact],
    alerts: &mut Vec<Alert>,
) -> Vec<BackupHealth> {
    let now = chrono::Utc::now().timestamp();
    let mut latest_artifact: HashMap<u32, &BackupArtifact> = HashMap::new();
    let mut latest_task: HashMap<u32, &TaskHealth> = HashMap::new();

    for artifact in artifacts {
        let replace = latest_artifact
            .get(&artifact.vmid)
            .map(|old| artifact.ctime > old.ctime)
            .unwrap_or(true);
        if replace {
            latest_artifact.insert(artifact.vmid, artifact);
        }
    }

    for task in tasks {
        if !is_backup_task(&task.worker_type) {
            continue;
        }
        if let Some(vmid) = task.vmid {
            let replace = latest_task
                .get(&vmid)
                .map(|old| task.start_time > old.start_time)
                .unwrap_or(true);
            if replace {
                latest_task.insert(vmid, task);
            }
        }
    }

    let mut rows = Vec::new();
    for guest in guests {
        let artifact = latest_artifact.get(&guest.vmid).copied();
        let task = latest_task.get(&guest.vmid).copied();
        let last_backup_ts = artifact.map(|a| a.ctime);
        let age_hours = last_backup_ts.map(|ts| (now.saturating_sub(ts)) / 3600);
        let latest_task_status = task.map(|t| t.status.clone()).unwrap_or_else(|| "none".to_string());
        let status = match age_hours {
            Some(age) if age >= cfg.backup_critical_hours as i64 => "critical",
            Some(age) if age >= cfg.backup_warn_hours as i64 => "warning",
            Some(_) => "ok",
            None => "critical",
        }.to_string();

        if status != "ok" {
            let summary = if let Some(age) = age_hours {
                format!("Guest {} ({}) latest backup artifact is {age}h old", guest.name, guest.vmid)
            } else {
                format!("Guest {} ({}) has no backup artifact found", guest.name, guest.vmid)
            };
            alerts.push(platform_alert(format!("backup:{}:{}", guest.vmid, status), &status, summary));
        }

        rows.push(BackupHealth {
            vmid: guest.vmid,
            name: guest.name.clone(),
            node: guest.node.clone(),
            kind: match guest.kind { GuestKind::Vm => "qemu", GuestKind::Lxc => "lxc" }.to_string(),
            last_backup_ts,
            age_hours,
            status,
            task_status: latest_task_status,
            size_bytes: artifact.and_then(|a| a.size_bytes),
            source: artifact
                .map(|a| format!("{}:{} ({})", a.node, a.storage, a.volid))
                .unwrap_or_else(|| "none".to_string()),
        });
    }
    rows
}

async fn collect_backup_artifacts(
    client: Arc<ProxmoxClient>,
    nodes: Arc<Vec<String>>,
) -> Vec<BackupArtifact> {
    let mut artifacts = Vec::new();

    for node in nodes.iter() {
        let storages = match client.storage_status(node).await {
            Ok(storages) => storages,
            Err(e) => {
                debug!("backup storage list {node}: {e}");
                continue;
            }
        };

        for storage in storages
            .iter()
            .filter(|s| s.enabled && s.active && s.content.split(',').any(|c| c.trim() == "backup"))
        {
            match client.storage_content(node, &storage.storage, "backup").await {
                Ok(rows) => {
                    artifacts.extend(rows.iter().filter_map(|row| parse_backup_artifact(row, node, &storage.storage)));
                }
                Err(e) => debug!("backup content {node}/{}: {e}", storage.storage),
            }
        }
    }

    artifacts.extend(scan_local_backup_artifacts().await);

    let mut dedup: HashMap<String, BackupArtifact> = HashMap::new();
    for artifact in artifacts {
        let key = if artifact.volid.is_empty() {
            format!("{}:{}:{}", artifact.node, artifact.vmid, artifact.ctime)
        } else {
            artifact.volid.clone()
        };
        dedup.entry(key).or_insert(artifact);
    }
    dedup.into_values().collect()
}

async fn collect_cluster() -> ClusterHealth {
    let pvecm = run_cmd("pvecm", &["status"]).await.unwrap_or_default();
    let quorum = if pvecm.contains("Quorate:          Yes") || pvecm.contains("Quorate: Yes") {
        "ok"
    } else if pvecm.is_empty() {
        "unknown"
    } else {
        "critical"
    }.to_string();
    let nodes = pvecm
        .lines()
        .filter(|line| line.contains("(local)") || line.trim_start().starts_with("0x"))
        .map(|line| line.trim().to_string())
        .collect();
    let ha_out = run_cmd("ha-manager", &["status", "--verbose"]).await.unwrap_or_default();
    let ha_resources = ha_out
        .lines()
        .filter(|line| line.contains("service") || line.contains("started") || line.contains("error"))
        .map(|line| json!({ "line": line }))
        .collect();
    ClusterHealth { quorum, nodes, detail: pvecm, ha_resources }
}

async fn collect_certs(cfg: &CertificateConfig, alerts: &mut Vec<Alert>) -> Vec<CertCheck> {
    let mut targets = vec![("proxmox-local".to_string(), "local-pveproxy-cert".to_string())];
    targets.extend(cfg.targets.iter().map(|t| (t.name.clone(), t.url.clone())));

    let mut rows = Vec::new();
    for (name, url) in targets {
        let check = if url == "local-pveproxy-cert" {
            check_local_cert(&name, cfg).await
        } else {
            check_remote_cert(&name, &url, cfg).await
        };
        if matches!(check.status.as_str(), "warning" | "critical") {
            alerts.push(platform_alert(
                format!("cert:{}", check.name),
                &check.status,
                format!("Certificate {}: {}", check.name, check.detail),
            ));
        }
        rows.push(check);
    }
    rows
}

async fn collect_security(
    guest_agents: &[GuestAgentHealth],
    alerts: &mut Vec<Alert>,
) -> Vec<SecurityCheck> {
    let mut checks = Vec::new();

    let sshd = tokio::fs::read_to_string("/etc/ssh/sshd_config").await.unwrap_or_default();
    let root_login = sshd.lines().rev().find_map(|line| {
        let line = line.trim();
        if line.starts_with('#') { return None; }
        line.strip_prefix("PermitRootLogin").map(|v| v.trim().to_string())
    }).unwrap_or_else(|| "default".to_string());
    checks.push(security_check(
        "root_login",
        "Root SSH login",
        if matches!(root_login.as_str(), "yes" | "prohibit-password" | "default") { "warning" } else { "ok" },
        &root_login,
        "Read-only sshd_config check",
    ));

    let pveversion = run_cmd("pveversion", &[]).await.unwrap_or_default();
    checks.push(security_check("pve_version", "PVE version", "info", pveversion.trim(), "Installed Proxmox version"));

    let repo_detail = read_repo_files().await;
    let repo_severity = if repo_detail.contains("pve-enterprise") && !repo_detail.contains("download.proxmox.com/debian/pve") {
        "warning"
    } else {
        "ok"
    };
    checks.push(security_check("repos", "Repository posture", repo_severity, &repo_detail, "Enterprise/no-subscription repo detection"));

    let fw = run_cmd("pve-firewall", &["status"]).await.unwrap_or_default();
    checks.push(security_check(
        "firewall",
        "PVE firewall",
        if fw.to_lowercase().contains("disabled") { "warning" } else { "ok" },
        fw.trim(),
        "Node firewall status",
    ));

    let no_agent = guest_agents
        .iter()
        .filter(|g| g.status != "ok")
        .count();
    checks.push(security_check(
        "guest_agent_visibility",
        "Guest visibility",
        if no_agent > 0 { "info" } else { "ok" },
        &format!("{no_agent} running QEMU guests have guest agent missing or not responding"),
        "Visibility posture",
    ));

    for check in &checks {
        if matches!(check.severity.as_str(), "warning" | "critical") {
            alerts.push(platform_alert(
                format!("security:{}", check.key),
                &check.severity,
                format!("Security check {}: {}", check.label, check.status),
            ));
        }
    }
    checks
}

async fn collect_guest_agent_health(
    client: Arc<ProxmoxClient>,
    guests: &[crate::proxmox_api::GuestStatus],
    alerts: &mut Vec<Alert>,
) -> Vec<GuestAgentHealth> {
    let mut rows = Vec::new();
    for guest in guests
        .iter()
        .filter(|g| matches!(g.kind, GuestKind::Vm) && g.status == "running")
    {
        let (status, detail) = match client.vm_agent_ping(&guest.node, guest.vmid).await {
            Ok(()) => ("ok".to_string(), "guest agent ping OK".to_string()),
            Err(e) => ("warning".to_string(), e.to_string()),
        };

        if status != "ok" {
            alerts.push(platform_alert(
                format!("guest_agent:{}:{}", guest.node, guest.vmid),
                "warning",
                format!("QEMU guest agent not responding for {} ({}) on {}: {}", guest.name, guest.vmid, guest.node, detail),
            ));
        }

        rows.push(GuestAgentHealth {
            vmid: guest.vmid,
            name: guest.name.clone(),
            node: guest.node.clone(),
            status,
            detail,
        });
    }
    rows
}

async fn collect_snapshots(
    cfg: &PlatformConfig,
    client: Arc<ProxmoxClient>,
    guests: &[crate::proxmox_api::GuestStatus],
    alerts: &mut Vec<Alert>,
) -> Vec<SnapshotHealth> {
    let now = chrono::Utc::now().timestamp();
    let mut rows = Vec::new();
    for guest in guests {
        let api_rows = match client.guest_snapshots(&guest.node, &guest.kind, guest.vmid).await {
            Ok(rows) => rows,
            Err(e) => {
                debug!("snapshot API {} {}: {e}", guest.node, guest.vmid);
                continue;
            }
        };
        let snapshots = parse_snapshot_api_rows(&api_rows, now);
        if snapshots.len() > cfg.snapshot_max_count {
            alerts.push(platform_alert(
                format!("snap_count:{}", guest.vmid),
                "warning",
                format!("Guest {} ({}) has {} snapshots", guest.name, guest.vmid, snapshots.len()),
            ));
        }
        let oldest_days = snapshots.iter().filter_map(|s| s.created_ts.map(|ts| (now - ts) / 86400)).max();
        if oldest_days.unwrap_or(0) >= cfg.snapshot_warn_days as i64 {
            alerts.push(platform_alert(
                format!("snap_old:{}", guest.vmid),
                "warning",
                format!("Guest {} ({}) has snapshots older than {} days", guest.name, guest.vmid, cfg.snapshot_warn_days),
            ));
        }
        if !snapshots.is_empty() {
            rows.push(SnapshotHealth {
                vmid: guest.vmid,
                name: guest.name.clone(),
                kind: match guest.kind { GuestKind::Vm => "qemu", GuestKind::Lxc => "lxc" }.to_string(),
                count: snapshots.len(),
                oldest_days,
                snapshots,
            });
        }
    }
    rows
}

async fn collect_ceph(alerts: &mut Vec<Alert>) -> CephHealth {
    let out = run_cmd("ceph", &["status", "--format", "json"]).await;
    let Ok(out) = out else {
        return CephHealth { installed: false, health: "not-installed".into(), detail: "ceph command unavailable".into(), osd_up: None, osd_total: None, mons: vec![] };
    };
    let value: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    let health = value.pointer("/health/status").and_then(Value::as_str).unwrap_or("UNKNOWN").to_string();
    if health != "HEALTH_OK" {
        alerts.push(platform_alert("ceph_health".into(), if health == "HEALTH_ERR" { "critical" } else { "warning" }, format!("Ceph health is {health}")));
    }
    let osd_up = value.pointer("/osdmap/osdmap/num_up_osds").and_then(Value::as_u64);
    let osd_total = value.pointer("/osdmap/osdmap/num_osds").and_then(Value::as_u64);
    if let (Some(up), Some(total)) = (osd_up, osd_total) {
        if up < total {
            alerts.push(platform_alert("ceph_osd_down".into(), "critical", format!("Ceph OSDs up {up}/{total}")));
        }
    }
    CephHealth {
        installed: true,
        health,
        detail: value.pointer("/health/checks").map(Value::to_string).unwrap_or_default(),
        osd_up,
        osd_total,
        mons: value.pointer("/quorum_names").and_then(Value::as_array).map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default(),
    }
}

async fn collect_thin_pools(cfg: &PlatformConfig, alerts: &mut Vec<Alert>) -> Vec<ThinPoolHealth> {
    let out = run_cmd("lvs", &["--reportformat", "json", "-o", "vg_name,lv_name,lv_attr,data_percent,metadata_percent"]).await.unwrap_or_default();
    let pools = parse_lvmthin_json(&out, cfg);
    for pool in &pools {
        if pool.status != "ok" {
            alerts.push(platform_alert(
                format!("thin:{}/{}", pool.vg, pool.lv),
                &pool.status,
                format!("LVM-thin {}/{}: data {:.1}%, metadata {:.1}%", pool.vg, pool.lv, pool.data_pct, pool.meta_pct),
            ));
        }
    }
    pools
}

fn classify_lvmthin_status(data_pct: f64, meta_pct: f64, cfg: &PlatformConfig) -> String {
    (if data_pct >= cfg.lvmthin_data_critical_pct || meta_pct >= cfg.lvmthin_metadata_critical_pct {
        "critical"
    } else if data_pct >= cfg.lvmthin_data_warn_pct || meta_pct >= cfg.lvmthin_metadata_warn_pct {
        "warning"
    } else {
        "ok"
    }).to_string()
}

async fn check_local_cert(name: &str, cfg: &CertificateConfig) -> CertCheck {
    let out = run_cmd("openssl", &["x509", "-in", "/etc/pve/local/pveproxy-ssl.pem", "-noout", "-enddate"]).await.unwrap_or_default();
    cert_from_not_after(name, "local-pveproxy-cert", &out, cfg)
}

async fn check_remote_cert(name: &str, url: &str, cfg: &CertificateConfig) -> CertCheck {
    let parsed = Url::parse(url);
    let Ok(parsed) = parsed else {
        return CertCheck { name: name.into(), url: url.into(), status: "critical".into(), days_remaining: None, expires_at: None, detail: "invalid URL".into() };
    };
    if parsed.scheme() != "https" {
        return CertCheck { name: name.into(), url: url.into(), status: "unknown".into(), days_remaining: None, expires_at: None, detail: "certificate checks require https URL".into() };
    }
    let Some(host) = parsed.host_str() else {
        return CertCheck { name: name.into(), url: url.into(), status: "critical".into(), days_remaining: None, expires_at: None, detail: "missing host".into() };
    };
    let port = parsed.port_or_known_default().unwrap_or(443).to_string();
    let connect = format!("{host}:{port}");
    match fetch_remote_cert_enddate(host, &connect).await {
        Ok(out) => cert_from_not_after(name, url, &out, cfg),
        Err(e) => CertCheck {
            name: name.into(),
            url: url.into(),
            status: "critical".into(),
            days_remaining: None,
            expires_at: None,
            detail: format!("certificate probe failed: {e}"),
        },
    }
}

fn cert_from_not_after(name: &str, url: &str, out: &str, cfg: &CertificateConfig) -> CertCheck {
    let raw = out.trim().strip_prefix("notAfter=").unwrap_or("").trim();
    if raw.is_empty() {
        return CertCheck { name: name.into(), url: url.into(), status: "unknown".into(), days_remaining: None, expires_at: None, detail: "certificate expiry unavailable".into() };
    }
    let parsed_ts = chrono::DateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y %Z")
        .map(|dt| dt.timestamp())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y GMT")
                .map(|dt| dt.and_utc().timestamp())
        })
        .ok();
    let days = parsed_ts.map(|ts| (ts - chrono::Utc::now().timestamp()) / 86400);
    let status = match days {
        Some(d) if d < 0 => "critical",
        Some(d) if d <= cfg.critical_days as i64 => "critical",
        Some(d) if d <= cfg.warn_days as i64 => "warning",
        Some(_) => "ok",
        None => "unknown",
    }.to_string();
    CertCheck {
        name: name.into(),
        url: url.into(),
        status,
        days_remaining: days,
        expires_at: parsed_ts.and_then(|ts| chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0).map(|d| d.to_rfc3339())),
        detail: raw.into(),
    }
}

fn security_check(key: &str, label: &str, severity: &str, status: &str, detail: &str) -> SecurityCheck {
    SecurityCheck {
        key: key.into(),
        label: label.into(),
        severity: severity.into(),
        status: status.into(),
        detail: detail.into(),
    }
}

async fn read_repo_files() -> String {
    let mut text = String::new();
    for path in ["/etc/apt/sources.list", "/etc/apt/sources.list.d/pve-enterprise.sources", "/etc/apt/sources.list.d/pve-no-subscription.sources"] {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            text.push_str(path);
            text.push('\n');
            text.push_str(&content);
            text.push('\n');
        }
    }
    if text.is_empty() { "no apt repo files readable".into() } else { text.lines().take(20).collect::<Vec<_>>().join(" | ") }
}

fn platform_alert(key: String, severity: &str, summary: String) -> Alert {
    Alert::PlatformIssue { key, severity: severity.into(), summary }
}

async fn run_cmd(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).output().await?;
    if !out.status.success() {
        anyhow::bail!("{} failed: {}", cmd, String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

async fn run_cmd_stdin(cmd: &str, args: &[&str], stdin_data: &[u8]) -> Result<String> {
    let mut child = Command::new(cmd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(stdin_data).await?;
    }

    let out = child.wait_with_output().await?;
    if !out.status.success() {
        anyhow::bail!("{} failed: {}", cmd, String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

async fn fetch_remote_cert_enddate(host: &str, connect: &str) -> Result<String> {
    let chain = run_cmd_stdin(
        "openssl",
        &["s_client", "-servername", host, "-connect", connect, "-showcerts"],
        b"\n",
    ).await?;
    let pem = extract_first_pem_cert(&chain).ok_or_else(|| anyhow::anyhow!("no PEM certificate in s_client output"))?;
    run_cmd_stdin("openssl", &["x509", "-noout", "-enddate"], pem.as_bytes()).await
}

fn extract_first_pem_cert(text: &str) -> Option<String> {
    let begin = text.find("-----BEGIN CERTIFICATE-----")?;
    let end = text[begin..].find("-----END CERTIFICATE-----")? + begin + "-----END CERTIFICATE-----".len();
    Some(format!("{}\n", &text[begin..end]))
}

fn pct_text(value: &str) -> f64 {
    value.trim().trim_end_matches('%').trim_end_matches('-').parse().unwrap_or(0.0)
}

fn parse_zfs_pools(list: &str, status: &str) -> Vec<ZfsPool> {
    let mut pools = Vec::new();
    for line in list.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 3 {
            continue;
        }
        let name = cols[0].to_string();
        let state = cols[1].to_string();
        let capacity_pct = pct_text(cols[2]);
        let fragmentation_pct = cols.get(3).map(|v| pct_text(v));
        let block = pool_status_block(status, &name);
        let scrub = parse_scrub(&block);
        let errors = parse_errors(&block);
        let (read_errors, write_errors, checksum_errors) = parse_vdev_errors(&block);
        pools.push(ZfsPool {
            name,
            state,
            capacity_pct,
            fragmentation_pct,
            scrub,
            errors,
            read_errors,
            write_errors,
            checksum_errors,
        });
    }
    pools
}

fn pool_status_block(status: &str, pool: &str) -> String {
    let mut capture = false;
    let mut lines = Vec::new();
    for line in status.lines() {
        if line.trim_start().starts_with("pool:") {
            if capture {
                break;
            }
            capture = line
                .trim_start()
                .strip_prefix("pool:")
                .map(|name| name.trim() == pool)
                .unwrap_or(false);
        }
        if capture {
            lines.push(line);
        }
    }
    lines.join("\n")
}

fn parse_scrub(block: &str) -> String {
    block
        .lines()
        .find(|line| line.trim_start().starts_with("scan:"))
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn parse_errors(block: &str) -> String {
    block
        .lines()
        .find(|line| line.trim_start().starts_with("errors:"))
        .map(|line| line.trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

fn parse_vdev_errors(block: &str) -> (u64, u64, u64) {
    let mut read = 0;
    let mut write = 0;
    let mut cksum = 0;
    for line in block.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() < 5 || cols[0] == "NAME" {
            continue;
        }
        let Some(state) = cols.get(1) else {
            continue;
        };
        if !matches!(*state, "ONLINE" | "DEGRADED" | "FAULTED" | "OFFLINE" | "UNAVAIL" | "REMOVED") {
            continue;
        }
        read = read.max(cols.get(cols.len().saturating_sub(3)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0));
        write = write.max(cols.get(cols.len().saturating_sub(2)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0));
        cksum = cksum.max(cols.last().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0));
    }
    (read, write, cksum)
}

fn scrub_has_errors(scrub: &str) -> bool {
    let lower = scrub.to_lowercase();
    if lower.contains("with 0 errors") && (lower.contains("repaired 0b") || lower.contains("repaired 0 bytes")) {
        return false;
    }
    lower.contains("with ") && lower.contains(" errors") && !lower.contains("with 0 errors")
        || lower.contains("repaired") && !lower.contains("repaired 0b") && !lower.contains("repaired 0 bytes")
}

fn parse_tasks_json(out: &str, now: i64) -> Vec<TaskHealth> {
    let value: Value = serde_json::from_str(out).unwrap_or(Value::Null);
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    rows.iter().take(250).map(|row| {
        let worker_type = str_field(row, "type").or_else(|| str_field(row, "worker_type")).unwrap_or_default();
        let status = str_field(row, "status").unwrap_or_else(|| if row.get("endtime").is_some() { "unknown".into() } else { "running".into() });
        let start_time = int_field(row, "starttime").unwrap_or(0);
        let end_time = int_field(row, "endtime");
        let duration_secs = end_time.unwrap_or(now).saturating_sub(start_time);
        TaskHealth {
            upid: str_field(row, "upid").unwrap_or_default(),
            node: str_field(row, "node").unwrap_or_default(),
            worker_type,
            vmid: int_field(row, "id").and_then(|v| u32::try_from(v).ok()),
            user: str_field(row, "user").unwrap_or_default(),
            status,
            start_time,
            end_time,
            duration_secs,
        }
    }).collect()
}

fn is_backup_task(worker_type: &str) -> bool {
    matches!(worker_type, "vzdump" | "backup" | "pbs") || worker_type.contains("backup")
}

fn parse_backup_artifact(row: &Value, node: &str, storage: &str) -> Option<BackupArtifact> {
    let volid = str_field(row, "volid").or_else(|| str_field(row, "volume")).unwrap_or_default();
    let vmid = int_field(row, "vmid")
        .and_then(|v| u32::try_from(v).ok())
        .or_else(|| parse_vmid_from_backup_name(&volid))?;
    let ctime = int_field(row, "ctime").or_else(|| parse_backup_timestamp(&volid)).unwrap_or(0);
    Some(BackupArtifact {
        vmid,
        node: node.to_string(),
        storage: storage.to_string(),
        volid,
        ctime,
        size_bytes: int_field(row, "size").and_then(|v| u64::try_from(v).ok()),
    })
}

async fn scan_local_backup_artifacts() -> Vec<BackupArtifact> {
    let mut artifacts = Vec::new();
    for dir in ["/var/lib/vz/dump", "/mnt/pve"] {
        scan_backup_dir(dir, &mut artifacts).await;
    }
    artifacts
}

async fn scan_backup_dir(path: &str, artifacts: &mut Vec<BackupArtifact>) {
    let mut stack = vec![std::path::PathBuf::from(path)];
    while let Some(dir) = stack.pop() {
        let mut entries = match tokio::fs::read_dir(&dir).await {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = entries.next_entry().await {
            let file_type = match entry.file_type().await {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            let path = entry.path();
            if file_type.is_dir() {
                if path.file_name().and_then(|n| n.to_str()) == Some("dump")
                    || dir.as_path() == std::path::Path::new("/mnt/pve")
                {
                    stack.push(path);
                }
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !name.starts_with("vzdump-") {
                continue;
            }
            let Some(vmid) = parse_vmid_from_backup_name(name) else {
                continue;
            };
            let metadata = entry.metadata().await.ok();
            artifacts.push(BackupArtifact {
                vmid,
                node: "local".to_string(),
                storage: "local-scan".to_string(),
                volid: path.display().to_string(),
                ctime: parse_backup_timestamp(name)
                    .or_else(|| metadata.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64))
                    .unwrap_or(0),
                size_bytes: metadata.map(|m| m.len()),
            });
        }
    }
}

fn parse_vmid_from_backup_name(name: &str) -> Option<u32> {
    let file = name.rsplit('/').next().unwrap_or(name);
    let parts: Vec<&str> = file.split('-').collect();
    if parts.len() < 3 || parts[0] != "vzdump" {
        return None;
    }
    parts[2].parse().ok()
}

fn parse_backup_timestamp(name: &str) -> Option<i64> {
    let file = name.rsplit('/').next().unwrap_or(name);
    let parts: Vec<&str> = file.split('-').collect();
    if parts.len() < 5 || parts[0] != "vzdump" {
        return None;
    }
    let raw = format!("{}-{}", parts[3], parts[4]);
    let trimmed = raw
        .trim_end_matches(".vma.zst")
        .trim_end_matches(".vma.lzo")
        .trim_end_matches(".vma.gz")
        .trim_end_matches(".tar.zst")
        .trim_end_matches(".tar.lzo")
        .trim_end_matches(".tar.gz");
    chrono::NaiveDateTime::parse_from_str(trimmed, "%Y_%m_%d-%H_%M_%S")
        .ok()
        .map(|dt| dt.and_utc().timestamp())
}

fn parse_snapshot_api_rows(rows: &[Value], now: i64) -> Vec<SnapshotInfo> {
    rows.iter().filter_map(|row| {
        let name = str_field(row, "name")?;
        if name == "current" {
            return None;
        }
        let created_ts = int_field(row, "snaptime").or_else(|| int_field(row, "ctime"));
        Some(SnapshotInfo {
            name,
            description: str_field(row, "description").unwrap_or_default(),
            created_ts,
            age_days: created_ts.map(|ts| now.saturating_sub(ts) / 86400),
        })
    }).collect()
}

fn parse_lvmthin_json(out: &str, cfg: &PlatformConfig) -> Vec<ThinPoolHealth> {
    let value: Value = serde_json::from_str(out).unwrap_or(Value::Null);
    let rows = value.pointer("/report/0/lv").and_then(Value::as_array).cloned().unwrap_or_default();
    rows.into_iter().filter_map(|row| {
        let attr = str_field(&row, "lv_attr").unwrap_or_default();
        if !attr.starts_with('t') {
            return None;
        }
        let vg = str_field(&row, "vg_name").unwrap_or_default();
        let lv = str_field(&row, "lv_name").unwrap_or_default();
        let data_pct = str_field(&row, "data_percent").and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let meta_pct = str_field(&row, "metadata_percent").and_then(|s| s.parse().ok()).unwrap_or(0.0);
        let status = classify_lvmthin_status(data_pct, meta_pct, cfg);
        Some(ThinPoolHealth { vg, lv, data_pct, meta_pct, status })
    }).collect()
}

fn update_platform_metrics(
    zfs: &[ZfsPool],
    backups: &[BackupHealth],
    tasks: &[TaskHealth],
    cluster: &ClusterHealth,
    ceph: &CephHealth,
    thin_pools: &[ThinPoolHealth],
    snapshots: &[SnapshotHealth],
    security: &[SecurityCheck],
    certificates: &[CertCheck],
    guest_agents: &[GuestAgentHealth],
) {
    for pool in zfs {
        let status = if pool.state == "ONLINE" { "ok" } else { "critical" };
        crate::exporter::prometheus::update_platform_health("zfs", &pool.name, status);
        crate::exporter::prometheus::update_platform_value("zfs", &pool.name, "capacity_pct", pool.capacity_pct);
        if let Some(frag) = pool.fragmentation_pct {
            crate::exporter::prometheus::update_platform_value("zfs", &pool.name, "fragmentation_pct", frag);
        }
    }
    for backup in backups {
        crate::exporter::prometheus::update_platform_health("backup", &backup.vmid.to_string(), &backup.status);
        if let Some(age) = backup.age_hours {
            crate::exporter::prometheus::update_platform_value("backup", &backup.vmid.to_string(), "age_hours", age as f64);
        }
    }
    let failed_tasks = tasks.iter().filter(|t| t.status.to_lowercase().contains("error") || t.status.to_lowercase().contains("fail")).count();
    let running_tasks = tasks.iter().filter(|t| t.end_time.is_none()).count();
    crate::exporter::prometheus::update_platform_value("tasks", "cluster", "failed_recent", failed_tasks as f64);
    crate::exporter::prometheus::update_platform_value("tasks", "cluster", "running", running_tasks as f64);
    crate::exporter::prometheus::update_platform_health("cluster", "quorum", &cluster.quorum);
    crate::exporter::prometheus::update_platform_health("ceph", "cluster", &ceph.health);
    for pool in thin_pools {
        let name = format!("{}/{}", pool.vg, pool.lv);
        crate::exporter::prometheus::update_platform_health("lvmthin", &name, &pool.status);
        crate::exporter::prometheus::update_platform_value("lvmthin", &name, "data_pct", pool.data_pct);
        crate::exporter::prometheus::update_platform_value("lvmthin", &name, "metadata_pct", pool.meta_pct);
    }
    for snapshot in snapshots {
        let status = if snapshot.oldest_days.unwrap_or(0) > 0 { "info" } else { "ok" };
        crate::exporter::prometheus::update_platform_health("snapshot", &snapshot.vmid.to_string(), status);
        crate::exporter::prometheus::update_platform_value("snapshot", &snapshot.vmid.to_string(), "count", snapshot.count as f64);
        if let Some(days) = snapshot.oldest_days {
            crate::exporter::prometheus::update_platform_value("snapshot", &snapshot.vmid.to_string(), "oldest_days", days as f64);
        }
    }
    for check in security {
        crate::exporter::prometheus::update_platform_health("security", &check.key, &check.severity);
    }
    for cert in certificates {
        crate::exporter::prometheus::update_platform_health("certificate", &cert.name, &cert.status);
        if let Some(days) = cert.days_remaining {
            crate::exporter::prometheus::update_platform_value("certificate", &cert.name, "days_remaining", days as f64);
        }
    }
    for agent in guest_agents {
        crate::exporter::prometheus::update_platform_health("guest_agent", &agent.vmid.to_string(), &agent.status);
    }
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())))
}

fn int_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_zfs_pool_status_and_errors() {
        let list = "rpool\tONLINE\t62%\t12%\ntank\tDEGRADED\t81%\t34%\n";
        let status = r#"
  pool: rpool
 state: ONLINE
  scan: scrub repaired 0B in 00:10:03 with 0 errors on Sun Apr 19 00:10:03 2026
config:

        NAME        STATE     READ WRITE CKSUM
        rpool       ONLINE       0     0     0
          sda3      ONLINE       0     0     0

errors: No known data errors

  pool: tank
 state: DEGRADED
  scan: scrub repaired 128K in 00:03:00 with 2 errors on Sun Apr 19 00:03:00 2026
config:

        NAME        STATE     READ WRITE CKSUM
        tank        DEGRADED     0     0     2
          sdb       ONLINE       0     0     0
          sdc       DEGRADED     0     0     2

errors: Permanent errors have been detected
"#;
        let pools = parse_zfs_pools(list, status);
        assert_eq!(pools.len(), 2);
        assert_eq!(pools[0].name, "rpool");
        assert_eq!(pools[0].state, "ONLINE");
        assert_eq!(pools[0].checksum_errors, 0);
        assert_eq!(pools[1].name, "tank");
        assert_eq!(pools[1].state, "DEGRADED");
        assert_eq!(pools[1].capacity_pct, 81.0);
        assert_eq!(pools[1].checksum_errors, 2);
        assert!(scrub_has_errors(&pools[1].scrub));
    }

    #[test]
    fn parses_lvmthin_thresholds_from_json() {
        let cfg = PlatformConfig::default();
        let json = r#"{
          "report": [{
            "lv": [
              {"vg_name":"pve","lv_name":"data","lv_attr":"twi-aotz--","data_percent":"86.2","metadata_percent":"12.5"},
              {"vg_name":"pve","lv_name":"root","lv_attr":"-wi-ao----","data_percent":"","metadata_percent":""}
            ]
          }]
        }"#;
        let pools = parse_lvmthin_json(json, &cfg);
        assert_eq!(pools.len(), 1);
        assert_eq!(pools[0].vg, "pve");
        assert_eq!(pools[0].lv, "data");
        assert_eq!(pools[0].status, "warning");
    }

    #[test]
    fn parses_task_history_rows() {
        let now = 1_700_000_600;
        let json = r#"[
          {"upid":"UPID:node:1","node":"node1","type":"vzdump","id":"101","user":"root@pam","status":"OK","starttime":1700000000,"endtime":1700000300},
          {"upid":"UPID:node:2","node":"node1","type":"qmigrate","id":"102","user":"root@pam","starttime":1700000000}
        ]"#;
        let tasks = parse_tasks_json(json, now);
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].vmid, Some(101));
        assert_eq!(tasks[0].duration_secs, 300);
        assert_eq!(tasks[1].status, "running");
        assert_eq!(tasks[1].duration_secs, 600);
    }

    #[test]
    fn parses_snapshot_api_metadata() {
        let now = 1_700_086_400;
        let rows: Vec<Value> = serde_json::from_str(r#"[
          {"name":"current"},
          {"name":"pre-upgrade","description":"before updates","snaptime":1700000000}
        ]"#).unwrap();
        let snapshots = parse_snapshot_api_rows(&rows, now);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].name, "pre-upgrade");
        assert_eq!(snapshots[0].age_days, Some(1));
    }

    #[test]
    fn parses_backup_artifact_from_storage_content() {
        let row: Value = serde_json::from_str(r#"{
          "volid": "backup:backup/vzdump-qemu-104-2026_04_24-12_30_00.vma.zst",
          "size": 123456,
          "ctime": 1777033800
        }"#).unwrap();
        let artifact = parse_backup_artifact(&row, "pve1", "backup").unwrap();
        assert_eq!(artifact.vmid, 104);
        assert_eq!(artifact.size_bytes, Some(123456));
        assert_eq!(artifact.ctime, 1777033800);
    }
}
