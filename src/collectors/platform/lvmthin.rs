use super::*;

pub(super) async fn collect_thin_pools(
    cfg: &PlatformConfig,
    alerts: &mut Vec<Alert>,
) -> Vec<ThinPoolHealth> {
    let out = run_cmd(
        "lvs",
        &[
            "--reportformat",
            "json",
            "-o",
            "vg_name,lv_name,lv_attr,data_percent,metadata_percent",
        ],
    )
    .await
    .unwrap_or_default();
    let pools = parse_lvmthin_json(&out, cfg);
    for pool in &pools {
        if pool.status != "ok" {
            alerts.push(platform_alert(
                format!("thin:{}/{}", pool.vg, pool.lv),
                &pool.status,
                format!(
                    "LVM-thin {}/{}: data {:.1}%, metadata {:.1}%",
                    pool.vg, pool.lv, pool.data_pct, pool.meta_pct
                ),
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
    })
    .to_string()
}

pub(super) fn parse_lvmthin_json(out: &str, cfg: &PlatformConfig) -> Vec<ThinPoolHealth> {
    let value: Value = serde_json::from_str(out).unwrap_or(Value::Null);
    let rows = value
        .pointer("/report/0/lv")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    rows.into_iter()
        .filter_map(|row| {
            let attr = str_field(&row, "lv_attr").unwrap_or_default();
            if !attr.starts_with('t') {
                return None;
            }
            let vg = str_field(&row, "vg_name").unwrap_or_default();
            let lv = str_field(&row, "lv_name").unwrap_or_default();
            let data_pct = str_field(&row, "data_percent")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let meta_pct = str_field(&row, "metadata_percent")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0.0);
            let status = classify_lvmthin_status(data_pct, meta_pct, cfg);
            Some(ThinPoolHealth {
                vg,
                lv,
                data_pct,
                meta_pct,
                status,
            })
        })
        .collect()
}
