use super::*;

pub(super) async fn collect_zfs(cfg: &PlatformConfig, alerts: &mut Vec<Alert>) -> Vec<ZfsPool> {
    let list = run_cmd(
        "zpool",
        &["list", "-H", "-o", "name,health,capacity,fragmentation"],
    )
    .await;
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
                if pool.capacity_pct >= 95.0 {
                    "critical"
                } else {
                    "warning"
                },
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
                format!(
                    "ZFS pool {} scrub reported errors: {}",
                    pool.name, pool.scrub
                ),
            ));
        }
    }
    pools
}

fn pct_text(value: &str) -> f64 {
    value
        .trim()
        .trim_end_matches('%')
        .trim_end_matches('-')
        .parse()
        .unwrap_or(0.0)
}

pub(super) fn parse_zfs_pools(list: &str, status: &str) -> Vec<ZfsPool> {
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
        if !matches!(
            *state,
            "ONLINE" | "DEGRADED" | "FAULTED" | "OFFLINE" | "UNAVAIL" | "REMOVED"
        ) {
            continue;
        }
        read = read.max(
            cols.get(cols.len().saturating_sub(3))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0),
        );
        write = write.max(
            cols.get(cols.len().saturating_sub(2))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0),
        );
        cksum = cksum.max(cols.last().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0));
    }
    (read, write, cksum)
}

pub(super) fn scrub_has_errors(scrub: &str) -> bool {
    let lower = scrub.to_lowercase();
    if lower.contains("with 0 errors")
        && (lower.contains("repaired 0b") || lower.contains("repaired 0 bytes"))
    {
        return false;
    }
    lower.contains("with ") && lower.contains(" errors") && !lower.contains("with 0 errors")
        || lower.contains("repaired")
            && !lower.contains("repaired 0b")
            && !lower.contains("repaired 0 bytes")
}
