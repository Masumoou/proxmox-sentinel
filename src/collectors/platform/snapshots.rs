use super::*;

pub(super) async fn collect_snapshots(
    cfg: &PlatformConfig,
    client: Arc<ProxmoxClient>,
    guests: &[crate::proxmox_api::GuestStatus],
    alerts: &mut Vec<Alert>,
) -> Vec<SnapshotHealth> {
    let now = chrono::Utc::now().timestamp();
    let mut rows = Vec::new();
    for guest in guests
        .iter()
        .filter(|g| !cfg.exclude_snapshot_vmids.contains(&g.vmid))
        .filter(|g| !(cfg.ignore_templates && g.template))
    {
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


pub(super) fn parse_snapshot_api_rows(rows: &[Value], now: i64) -> Vec<SnapshotInfo> {
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
