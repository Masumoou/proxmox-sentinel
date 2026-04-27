use super::*;

pub(super) async fn collect_ceph(alerts: &mut Vec<Alert>) -> CephHealth {
    let out = run_cmd("ceph", &["status", "--format", "json"]).await;
    let Ok(out) = out else {
        return CephHealth {
            installed: false,
            health: "not-installed".into(),
            detail: "ceph command unavailable".into(),
            osd_up: None,
            osd_total: None,
            mons: vec![],
        };
    };
    let health = parse_ceph_json(&out);
    if health.health != "HEALTH_OK" {
        alerts.push(platform_alert(
            "ceph_health".into(),
            if health.health == "HEALTH_ERR" {
                "critical"
            } else {
                "warning"
            },
            format!("Ceph health is {}", health.health),
        ));
    }
    if let (Some(up), Some(total)) = (health.osd_up, health.osd_total) {
        if up < total {
            alerts.push(platform_alert(
                "ceph_osd_down".into(),
                "critical",
                format!("Ceph OSDs up {up}/{total}"),
            ));
        }
    }
    health
}

pub(super) fn parse_ceph_json(out: &str) -> CephHealth {
    let value: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    let health = value
        .pointer("/health/status")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN")
        .to_string();
    let osd_up = value
        .pointer("/osdmap/osdmap/num_up_osds")
        .and_then(Value::as_u64);
    let osd_total = value
        .pointer("/osdmap/osdmap/num_osds")
        .and_then(Value::as_u64);
    CephHealth {
        installed: true,
        health,
        detail: value
            .pointer("/health/checks")
            .map(Value::to_string)
            .unwrap_or_default(),
        osd_up,
        osd_total,
        mons: value
            .pointer("/quorum_names")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default(),
    }
}
