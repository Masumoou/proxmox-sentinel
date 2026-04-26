use super::*;

pub(super) async fn collect_tasks(
    cfg: &PlatformConfig,
    alerts: &mut Vec<Alert>,
) -> Vec<TaskHealth> {
    let out = run_cmd(
        "pvesh",
        &["get", "/cluster/tasks", "--output-format", "json"],
    )
    .await
    .unwrap_or_default();
    let now = chrono::Utc::now().timestamp();
    let tasks = parse_tasks_json(&out, now);
    for task in &tasks {
        if task.status.to_lowercase().contains("error")
            || task.status.to_lowercase().contains("fail")
        {
            alerts.push(platform_alert(
                format!("task_failed:{}", task.upid),
                "critical",
                format!(
                    "Proxmox task {} failed on {}: {}",
                    task.worker_type, task.node, task.status
                ),
            ));
        }
        if task.end_time.is_none()
            && task.duration_secs > (cfg.task_long_running_minutes as i64 * 60)
        {
            alerts.push(platform_alert(
                format!("task_long:{}", task.upid),
                "warning",
                format!(
                    "Proxmox task {} on {} has been running for {} minutes",
                    task.worker_type,
                    task.node,
                    task.duration_secs / 60
                ),
            ));
        }
    }
    tasks
}

pub(super) fn parse_tasks_json(out: &str, now: i64) -> Vec<TaskHealth> {
    let value: Value = serde_json::from_str(out).unwrap_or(Value::Null);
    let Some(rows) = value.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .take(250)
        .map(|row| {
            let worker_type = str_field(row, "type")
                .or_else(|| str_field(row, "worker_type"))
                .unwrap_or_default();
            let status = str_field(row, "status").unwrap_or_else(|| {
                if row.get("endtime").is_some() {
                    "unknown".into()
                } else {
                    "running".into()
                }
            });
            let start_time = int_field(row, "starttime").unwrap_or(0);
            let end_time = int_field(row, "endtime");
            let duration_secs = end_time.unwrap_or(now).saturating_sub(start_time);
            TaskHealth {
                upid: str_field(row, "upid").unwrap_or_default(),
                node: str_field(row, "node").unwrap_or_default(),
                worker_type,
                vmid: int_field(row, "id").and_then(|v| u32::try_from(v).ok()),
                user: str_field(row, "user").unwrap_or_default(),
                status,
                start_time,
                end_time,
                duration_secs,
            }
        })
        .collect()
}

pub(super) fn is_backup_task(worker_type: &str) -> bool {
    matches!(worker_type, "vzdump" | "backup" | "pbs") || worker_type.contains("backup")
}
