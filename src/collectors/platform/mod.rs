use crate::alerts::{Alert, AlertDispatcher};
use crate::config::{BackupPolicyConfig, CertificateConfig, PlatformConfig};
use crate::proxmox_api::{GuestKind, ProxmoxClient};
use anyhow::Result;
use reqwest::Url;
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio::time::{Duration, interval};
use tracing::debug;

mod backups;
mod ceph;
mod certs;
mod cluster;
mod guest_agent;
mod lvmthin;
mod metrics;
mod security;
mod snapshots;
mod tasks;
mod zfs;

use backups::{collect_backup_artifacts, collect_backups};
use ceph::collect_ceph;
use certs::collect_certs;
use cluster::collect_cluster;
use guest_agent::collect_guest_agent_health;
use lvmthin::collect_thin_pools;
use metrics::update_platform_metrics;
use security::collect_security;
use snapshots::collect_snapshots;
use tasks::collect_tasks;
use zfs::collect_zfs;

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
    backup_policy: BackupPolicyConfig,
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
        let guest_agents =
            collect_guest_agent_health(&cfg, client.clone(), &guests, &mut alerts).await;
        let backup_artifacts = collect_backup_artifacts(client.clone(), nodes.clone()).await;
        let backups = collect_backups(
            &cfg,
            &backup_policy,
            &guests,
            &tasks,
            &backup_artifacts,
            &mut alerts,
        )
        .await;
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

        let _ = ws_tx.send(
            json!({
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
            })
            .to_string(),
        );
    }
}

async fn collect_all_guests(
    client: Arc<ProxmoxClient>,
    nodes: Arc<Vec<String>>,
) -> Vec<crate::proxmox_api::GuestStatus> {
    let mut guests = Vec::new();
    for node in nodes.iter() {
        match client.list_guests(node).await {
            Ok(mut found) => guests.append(&mut found),
            Err(e) => debug!("platform guest list {node}: {e}"),
        }
    }
    guests
}

fn platform_alert(key: String, severity: &str, summary: String) -> Alert {
    Alert::PlatformIssue {
        key,
        severity: severity.into(),
        summary,
    }
}

async fn run_cmd(cmd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new(cmd).args(args).output().await?;
    if !out.status.success() {
        anyhow::bail!("{} failed: {}", cmd, String::from_utf8_lossy(&out.stderr));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(|v| {
        v.as_str()
            .map(str::to_string)
            .or_else(|| v.as_i64().map(|n| n.to_string()))
    })
}

fn int_field(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(|v| v.as_i64().or_else(|| v.as_str()?.parse().ok()))
}

#[cfg(test)]
mod tests {
    use super::backups::parse_backup_artifact;
    use super::ceph::parse_ceph_json;
    use super::lvmthin::parse_lvmthin_json;
    use super::snapshots::parse_snapshot_api_rows;
    use super::tasks::parse_tasks_json;
    use super::zfs::{parse_zfs_pools, scrub_has_errors};
    use super::*;

    #[test]
    fn parses_zfs_pool_status_and_errors() {
        let list = include_str!("../../../tests/fixtures/platform/zpool_list.tsv");
        let status = include_str!("../../../tests/fixtures/platform/zpool_status.txt");
        let pools = parse_zfs_pools(list, status);
        assert_eq!(pools.len(), 3);
        assert_eq!(pools[0].name, "rpool");
        assert_eq!(pools[0].state, "ONLINE");
        assert_eq!(pools[0].checksum_errors, 0);
        assert_eq!(pools[1].name, "tank");
        assert_eq!(pools[1].state, "DEGRADED");
        assert_eq!(pools[1].capacity_pct, 81.0);
        assert_eq!(pools[1].checksum_errors, 2);
        assert!(scrub_has_errors(&pools[1].scrub));
        assert_eq!(pools[2].name, "scratch");
        assert_eq!(pools[2].state, "FAULTED");
        assert_eq!(pools[2].read_errors, 4);
    }

    #[test]
    fn parses_lvmthin_thresholds_from_json() {
        let cfg = PlatformConfig::default();
        let json = include_str!("../../../tests/fixtures/platform/lvs.json");
        let pools = parse_lvmthin_json(json, &cfg);
        assert_eq!(pools.len(), 2);
        assert_eq!(pools[0].vg, "pve");
        assert_eq!(pools[0].lv, "data");
        assert_eq!(pools[0].status, "warning");
        assert_eq!(pools[1].lv, "vmdata");
        assert_eq!(pools[1].status, "critical");
    }

    #[test]
    fn parses_task_history_rows() {
        let now = 1_700_000_600;
        let json = include_str!("../../../tests/fixtures/platform/pvesh_tasks.json");
        let tasks = parse_tasks_json(json, now);
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].vmid, Some(101));
        assert_eq!(tasks[0].duration_secs, 300);
        assert_eq!(tasks[1].status, "running");
        assert_eq!(tasks[1].duration_secs, 600);
        assert!(tasks[2].status.contains("ERROR"));
    }

    #[test]
    fn parses_snapshot_api_metadata() {
        let now = 1_700_086_400;
        let rows: Vec<Value> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/platform/snapshots.json"
        ))
        .unwrap();
        let snapshots = parse_snapshot_api_rows(&rows, now);
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].name, "pre-upgrade");
        assert_eq!(snapshots[0].age_days, Some(1));
        assert_eq!(snapshots[1].name, "before-migration");
        assert_eq!(snapshots[1].age_days, Some(2));
    }

    #[test]
    fn parses_backup_artifact_from_storage_content() {
        let rows: Vec<Value> = serde_json::from_str(include_str!(
            "../../../tests/fixtures/platform/storage_content_backups.json"
        ))
        .unwrap();
        let artifact = parse_backup_artifact(&rows[0], "pve1", "backup").unwrap();
        assert_eq!(artifact.vmid, 104);
        assert_eq!(artifact.size_bytes, Some(123456));
        assert_eq!(artifact.ctime, 1777033800);
        let parsed_from_name = parse_backup_artifact(&rows[1], "pve1", "backup").unwrap();
        assert_eq!(parsed_from_name.vmid, 105);
        assert!(parsed_from_name.ctime > 0);
    }

    #[test]
    fn parses_ceph_status_json() {
        let ceph = parse_ceph_json(include_str!(
            "../../../tests/fixtures/platform/ceph_status.json"
        ));
        assert!(ceph.installed);
        assert_eq!(ceph.health, "HEALTH_WARN");
        assert_eq!(ceph.osd_up, Some(2));
        assert_eq!(ceph.osd_total, Some(3));
        assert_eq!(ceph.mons, vec!["pve1".to_string(), "pve2".to_string()]);
    }
}
