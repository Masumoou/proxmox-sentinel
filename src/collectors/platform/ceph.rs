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
            warnings: vec![],
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
    let value: Value = serde_json::from_str(out).unwrap_or(Value::Null);
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
    let warnings = parse_ceph_warnings(&value);
    let detail = warnings
        .iter()
        .map(|warning| format!("{}: {}", warning.name, warning.message))
        .collect::<Vec<_>>()
        .join("; ");

    CephHealth {
        installed: true,
        health,
        detail,
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
        warnings,
    }
}

fn parse_ceph_warnings(value: &Value) -> Vec<CephWarning> {
    let Some(checks) = value.pointer("/health/checks").and_then(Value::as_object) else {
        return Vec::new();
    };
    checks
        .iter()
        .map(|(name, check)| {
            let severity = check
                .get("severity")
                .and_then(Value::as_str)
                .unwrap_or("warning")
                .to_string();
            let summary = check
                .get("summary")
                .and_then(Value::as_object)
                .and_then(|summary| summary.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let detail = check
                .get("detail")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(|item| {
                            item.get("message")
                                .and_then(Value::as_str)
                                .map(str::to_string)
                        })
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .unwrap_or_default();
            CephWarning {
                name: name.clone(),
                severity,
                message: summary,
                detail,
            }
        })
        .collect()
}
