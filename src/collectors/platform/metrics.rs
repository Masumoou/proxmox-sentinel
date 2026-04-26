use super::*;

pub(super) fn update_platform_metrics(
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