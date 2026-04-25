use crate::alerts::{Alert, AlertDispatcher};
use crate::config::{CertificateConfig, PlatformConfig};
use crate::proxmox_api::{GuestKind, ProxmoxClient};
use anyhow::Result;
use reqwest::Url;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{debug, warn};

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
        let thin_pools = collect_thin_pools(&mut alerts).await;
        let guests = collect_all_guests(client.clone(), nodes.clone()).await;
        let backups = collect_backups(&cfg, &guests, &tasks, &mut alerts).await;
        let snapshots = collect_snapshots(&cfg, &guests, &mut alerts).await;
        let security = if cfg.security_enabled {
            collect_security(&guests, &mut alerts).await
        } else {
            Vec::new()
        };
        let certificates = collect_certs(&cert_cfg, &mut alerts).await;

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
        let block = pool_status_block(&status, &name);
        let scrub = parse_scrub(&block);
        let errors = parse_errors(&block);
        let (read_errors, write_errors, checksum_errors) = parse_vdev_errors(&block);

        if state != "ONLINE" {
            alerts.push(platform_alert(
                format!("zfs_state:{name}"),
                "critical",
                format!("ZFS pool {name} is {state}"),
            ));
        }
        if capacity_pct >= cfg.zfs_usage_threshold {
            alerts.push(platform_alert(
                format!("zfs_usage:{name}"),
                if capacity_pct >= 95.0 { "critical" } else { "warning" },
                format!("ZFS pool {name} usage at {capacity_pct:.1}%"),
            ));
        }
        if checksum_errors > 0 || read_errors > 0 || write_errors > 0 {
            alerts.push(platform_alert(
                format!("zfs_errors:{name}"),
                "critical",
                format!("ZFS pool {name} has device errors: read={read_errors} write={write_errors} checksum={checksum_errors}"),
            ));
        }
        if scrub.to_lowercase().contains("repaired") && !scrub.contains("0B") {
            alerts.push(platform_alert(
                format!("zfs_scrub:{name}"),
                "warning",
                format!("ZFS pool {name} scrub reported repairs: {scrub}"),
            ));
        }

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

async fn collect_tasks(cfg: &PlatformConfig, alerts: &mut Vec<Alert>) -> Vec<TaskHealth> {
    let out = run_cmd("pvesh", &["get", "/cluster/tasks", "--output-format", "json"]).await.unwrap_or_default();
    let value: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    let now = chrono::Utc::now().timestamp();
    let mut tasks = Vec::new();
    for row in rows.iter().take(250) {
        let worker_type = str_field(row, "type").or_else(|| str_field(row, "worker_type")).unwrap_or_default();
        let status = str_field(row, "status").unwrap_or_else(|| if row.get("endtime").is_some() { "unknown".into() } else { "running".into() });
        let start_time = int_field(row, "starttime").unwrap_or(0);
        let end_time = int_field(row, "endtime");
        let duration_secs = end_time.unwrap_or(now).saturating_sub(start_time);
        let upid = str_field(row, "upid").unwrap_or_default();
        let vmid = int_field(row, "id").and_then(|v| u32::try_from(v).ok());
        let node = str_field(row, "node").unwrap_or_default();

        if status.to_lowercase().contains("error") || status.to_lowercase().contains("fail") {
            alerts.push(platform_alert(
                format!("task_failed:{upid}"),
                "critical",
                format!("Proxmox task {worker_type} failed on {node}: {status}"),
            ));
        }
        if end_time.is_none() && duration_secs > (cfg.task_long_running_minutes as i64 * 60) {
            alerts.push(platform_alert(
                format!("task_long:{upid}"),
                "warning",
                format!("Proxmox task {worker_type} on {node} has been running for {} minutes", duration_secs / 60),
            ));
        }

        tasks.push(TaskHealth {
            upid,
            node,
            worker_type,
            vmid,
            user: str_field(row, "user").unwrap_or_default(),
            status,
            start_time,
            end_time,
            duration_secs,
        });
    }
    tasks
}

async fn collect_backups(
    cfg: &PlatformConfig,
    guests: &[crate::proxmox_api::GuestStatus],
    tasks: &[TaskHealth],
    alerts: &mut Vec<Alert>,
) -> Vec<BackupHealth> {
    let now = chrono::Utc::now().timestamp();
    let mut latest: HashMap<u32, &TaskHealth> = HashMap::new();

    for task in tasks {
        if !matches!(task.worker_type.as_str(), "vzdump" | "backup" | "pbs") {
            continue;
        }
        if let Some(vmid) = task.vmid {
            let replace = latest
                .get(&vmid)
                .map(|old| task.start_time > old.start_time)
                .unwrap_or(true);
            if replace {
                latest.insert(vmid, task);
            }
        }
    }

    let mut rows = Vec::new();
    for guest in guests {
        let task = latest.get(&guest.vmid).copied();
        let last_backup_ts = task.map(|t| t.end_time.unwrap_or(t.start_time));
        let age_hours = last_backup_ts.map(|ts| (now.saturating_sub(ts)) / 3600);
        let status = match (task, age_hours) {
            (Some(t), _) if t.status.to_lowercase().contains("error") || t.status.to_lowercase().contains("fail") => "critical",
            (_, Some(age)) if age >= cfg.backup_critical_hours as i64 => "critical",
            (_, Some(age)) if age >= cfg.backup_warn_hours as i64 => "warning",
            (Some(_), _) => "ok",
            (None, _) => "critical",
        }.to_string();

        if status != "ok" {
            let summary = if let Some(age) = age_hours {
                format!("Guest {} ({}) backup age is {age}h", guest.name, guest.vmid)
            } else {
                format!("Guest {} ({}) has no known backup task", guest.name, guest.vmid)
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
            task_status: task.map(|t| t.status.clone()).unwrap_or_else(|| "never".to_string()),
            size_bytes: None,
        });
    }
    rows
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
    guests: &[crate::proxmox_api::GuestStatus],
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

    let no_agent = guests
        .iter()
        .filter(|g| matches!(g.kind, GuestKind::Vm) && g.status == "running")
        .count();
    checks.push(security_check(
        "guest_agent_visibility",
        "Guest visibility",
        if no_agent > 0 { "info" } else { "ok" },
        &format!("{no_agent} running QEMU guests require guest-agent/SSH for deep service visibility"),
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

async fn collect_snapshots(
    cfg: &PlatformConfig,
    guests: &[crate::proxmox_api::GuestStatus],
    alerts: &mut Vec<Alert>,
) -> Vec<SnapshotHealth> {
    let now = chrono::Utc::now().timestamp();
    let mut rows = Vec::new();
    for guest in guests {
        let cmd = match guest.kind { GuestKind::Vm => "qm", GuestKind::Lxc => "pct" };
        let out = run_cmd(cmd, &["listsnapshot", &guest.vmid.to_string()]).await.unwrap_or_default();
        let snapshots: Vec<SnapshotInfo> = out
            .lines()
            .filter(|line| !line.contains("current") && !line.trim().is_empty() && !line.contains("Name"))
            .filter_map(|line| {
                let name = line
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .trim_matches(|c| c == '`' || c == '-' || c == '>' || c == '|')
                    .to_string();
                if name.is_empty() || name == "current" {
                    return None;
                }
                Some(SnapshotInfo { name, description: line.trim().to_string(), created_ts: None, age_days: None })
            })
            .collect();
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

async fn collect_thin_pools(alerts: &mut Vec<Alert>) -> Vec<ThinPoolHealth> {
    let out = run_cmd("lvs", &["--reportformat", "json", "-o", "vg_name,lv_name,lv_attr,data_percent,metadata_percent"]).await.unwrap_or_default();
    let value: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
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
        let status = if data_pct >= 95.0 || meta_pct >= 90.0 { "critical" } else if data_pct >= 85.0 || meta_pct >= 75.0 { "warning" } else { "ok" }.to_string();
        if status != "ok" {
            alerts.push(platform_alert(format!("thin:{vg}/{lv}"), &status, format!("LVM-thin {vg}/{lv}: data {data_pct:.1}%, metadata {meta_pct:.1}%")));
        }
        Some(ThinPoolHealth { vg, lv, data_pct, meta_pct, status })
    }).collect()
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
    let Some(host) = parsed.host_str() else {
        return CertCheck { name: name.into(), url: url.into(), status: "critical".into(), days_remaining: None, expires_at: None, detail: "missing host".into() };
    };
    let port = parsed.port_or_known_default().unwrap_or(443).to_string();
    let connect = format!("{host}:{port}");
    let cmd = format!("echo | openssl s_client -servername {} -connect {} 2>/dev/null | openssl x509 -noout -enddate", shell_arg(host), shell_arg(&connect));
    let out = run_shell(&cmd).await.unwrap_or_default();
    cert_from_not_after(name, url, &out, cfg)
}

fn cert_from_not_after(name: &str, url: &str, out: &str, cfg: &CertificateConfig) -> CertCheck {
    let raw = out.trim().strip_prefix("notAfter=").unwrap_or("").trim();
    if raw.is_empty() {
        return CertCheck { name: name.into(), url: url.into(), status: "unknown".into(), days_remaining: None, expires_at: None, detail: "certificate expiry unavailable".into() };
    }
    let parsed = chrono::DateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y %Z").ok();
    let days = parsed.map(|dt| (dt.timestamp() - chrono::Utc::now().timestamp()) / 86400);
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
        expires_at: parsed.map(|d| d.to_rfc3339()),
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

async fn run_shell(script: &str) -> Result<String> {
    run_cmd("sh", &["-lc", script]).await
}

fn pct_text(value: &str) -> f64 {
    value.trim().trim_end_matches('%').trim_end_matches('-').parse().unwrap_or(0.0)
}

fn pool_status_block(status: &str, pool: &str) -> String {
    let mut capture = false;
    let mut lines = Vec::new();
    for line in status.lines() {
        if line.trim_start().starts_with("pool:") {
            capture = line.contains(pool);
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
        if cols.len() >= 5 && cols[1].chars().all(|c| c.is_ascii_digit()) && cols[2].chars().all(|c| c.is_ascii_digit()) {
            read += cols.get(cols.len().saturating_sub(3)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            write += cols.get(cols.len().saturating_sub(2)).and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
            cksum += cols.last().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        }
    }
    (read, write, cksum)
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| v.as_str().map(str::to_string).or_else(|| v.as_i64().map(|n| n.to_string())))
}

fn int_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
}

fn shell_arg(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
