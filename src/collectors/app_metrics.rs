// src/collectors/app_metrics.rs

use crate::alerts::{Alert, AlertDispatcher};
use crate::config::AppMetricsConfig;
use crate::exporter::prometheus::update_app_metrics;
use crate::storage::Storage;
use serde_json::Value;
use std::sync::Arc;
use tokio::process::Command;
use tokio::sync::broadcast;
use tokio::time::{Duration, interval};
use tracing::{info, warn};

pub async fn run_collector(
    cfg: AppMetricsConfig,
    storage: Arc<Storage>,
    mut dispatcher: AlertDispatcher,
    ws_tx: broadcast::Sender<String>,
) {
    if !cfg.enabled {
        return;
    }

    info!("Starting App Metrics collector: {}", cfg.name);
    let mut interval = interval(Duration::from_secs(cfg.interval_secs));
    let mut failure_count = 0;

    loop {
        interval.tick().await;

        match collect_metrics(&cfg).await {
            Ok(json_val) => {
                failure_count = 0;
                process_metrics(&cfg, json_val, &storage, &ws_tx).await;
            }
            Err(e) => {
                failure_count += 1;
                warn!(
                    "App Metrics collection failed for {}: {} (failure {}/3)",
                    cfg.name, e, failure_count
                );
                if failure_count >= 3 {
                    dispatcher
                        .dispatch(Alert::AppDown {
                            name: cfg.name.clone(),
                        })
                        .await;
                }
            }
        }
    }
}

async fn collect_metrics(cfg: &AppMetricsConfig) -> anyhow::Result<Value> {
    match cfg.kind.as_str() {
        "nextcloud_occ" => {
            let vmid = cfg
                .target_vmid
                .ok_or_else(|| anyhow::anyhow!("nextcloud_occ requires target_vmid"))?;
            let occ_path = cfg
                .command
                .as_deref()
                .unwrap_or("/var/www/html/nextcloud/occ");

            let output = Command::new("pct")
                .args([
                    "exec",
                    &vmid.to_string(),
                    "--",
                    "sudo",
                    "-u",
                    "www-data",
                    "php",
                    occ_path,
                    "serverinfo",
                    "--output=json",
                ])
                .output()
                .await?;

            if !output.status.success() {
                anyhow::bail!(
                    "occ command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            let val: Value = serde_json::from_slice(&output.stdout)?;
            Ok(val)
        }
        "http_json" => {
            let url = cfg
                .endpoint_url
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("http_json requires endpoint_url"))?;
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .build()?;
            let res = client.get(url).send().await?;
            let val: Value = res.json().await?;
            Ok(val)
        }
        "shell_json" => {
            let cmd = cfg
                .command
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("shell_json requires command"))?;
            let output = if let Some(vmid) = cfg.target_vmid {
                Command::new("pct")
                    .args(["exec", &vmid.to_string(), "--", "sh", "-c", cmd])
                    .output()
                    .await?
            } else {
                Command::new("sh").args(["-c", cmd]).output().await?
            };

            if !output.status.success() {
                anyhow::bail!(
                    "shell command failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }

            let val: Value = serde_json::from_slice(&output.stdout)?;
            Ok(val)
        }
        _ => anyhow::bail!("Unknown app metric kind: {}", cfg.kind),
    }
}

async fn process_metrics(
    cfg: &AppMetricsConfig,
    val: Value,
    storage: &Arc<Storage>,
    ws_tx: &broadcast::Sender<String>,
) {
    let mut mapped_metrics = serde_json::Map::new();

    for mapping in &cfg.json_path_mappings {
        if let Some(metric_val) = get_json_path(&val, &mapping.json_path) {
            if let Some(num) = metric_val.as_f64() {
                // Save to storage
                if let Err(e) = storage.insert_app_metric(&cfg.name, &mapping.metric_name, num) {
                    warn!("Failed to save app metric {}: {}", mapping.metric_name, e);
                }

                // Update prometheus
                update_app_metrics(&cfg.name, &mapping.metric_name, num);

                // Add to mapped result for WS
                mapped_metrics.insert(
                    mapping.metric_name.clone(),
                    serde_json::json!({
                        "value": num,
                        "label": mapping.label,
                        "unit": mapping.unit
                    }),
                );
            }
        }
    }

    // Broadcast update
    let event = serde_json::json!({
        "type": "app_metrics_update",
        "app": cfg.name,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "metrics": mapped_metrics
    });
    let _ = ws_tx.send(event.to_string());
}

fn get_json_path<'a>(val: &'a Value, path: &str) -> Option<&'a Value> {
    let mut curr = val;
    for part in path.split('.') {
        curr = curr.get(part)?;
    }
    Some(curr)
}
