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
    use super::lvmthin::parse_lvmthin_json;
    use super::snapshots::parse_snapshot_api_rows;
    use super::tasks::parse_tasks_json;
    use super::zfs::{parse_zfs_pools, scrub_has_errors};
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
        let rows: Vec<Value> = serde_json::from_str(
            r#"[
          {"name":"current"},
          {"name":"pre-upgrade","description":"before updates","snaptime":1700000000}
        ]"#,
        )
        .unwrap();
        let snapshots = parse_snapshot_api_rows(&rows, now);
        assert_eq!(snapshots.len(), 1);
        assert_eq!(snapshots[0].name, "pre-upgrade");
        assert_eq!(snapshots[0].age_days, Some(1));
    }

    #[test]
    fn parses_backup_artifact_from_storage_content() {
        let row: Value = serde_json::from_str(
            r#"{
          "volid": "backup:backup/vzdump-qemu-104-2026_04_24-12_30_00.vma.zst",
          "size": 123456,
          "ctime": 1777033800
        }"#,
        )
        .unwrap();
        let artifact = parse_backup_artifact(&row, "pve1", "backup").unwrap();
        assert_eq!(artifact.vmid, 104);
        assert_eq!(artifact.size_bytes, Some(123456));
        assert_eq!(artifact.ctime, 1777033800);
    }
}
