use super::*;

pub(super) async fn collect_cluster() -> ClusterHealth {
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