// src/collectors/file_activity.rs
//
// Periodically watches specified log files using tail -F
// and matches a regex to detect security events or anomalies.

use crate::config::FileActivityConfig;
use regex::Regex;
use serde_json::json;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::broadcast;
use tracing::{info, warn};

pub async fn run_collector(cfg: FileActivityConfig, ws_tx: broadcast::Sender<String>) {
    if !cfg.enabled || cfg.watch_paths.is_empty() {
        return;
    }

    let re = match Regex::new(&cfg.access_log_regex) {
        Ok(r) => r,
        Err(e) => {
            warn!("Invalid access_log_regex in config: {}", e);
            return;
        }
    };

    info!("Starting File Activity collector on {} paths", cfg.watch_paths.len());

    for path in cfg.watch_paths {
        let ws_tx = ws_tx.clone();
        let re = re.clone();
        let path_clone = path.clone();

        tokio::spawn(async move {
            let child = Command::new("tail")
                .arg("-F")
                .arg("-n")
                .arg("0") // start at end
                .arg(&path_clone)
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();

            let mut child_proc = match child {
                Ok(c) => c,
                Err(e) => {
                    warn!("Failed to spawn tail for {}: {}", path_clone, e);
                    return;
                }
            };

            let stdout = child_proc.stdout.take().unwrap();
            let mut reader = BufReader::new(stdout).lines();

            while let Ok(Some(line)) = reader.next_line().await {
                if let Some(caps) = re.captures(&line) {
                    let mut match_data = serde_json::Map::new();
                    // Extract named capture groups
                    for name in re.capture_names().flatten() {
                        if let Some(m) = caps.name(name) {
                            match_data.insert(name.to_string(), json!(m.as_str()));
                        }
                    }

                    let event = json!({
                        "type": "security_event",
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "file": path_clone,
                        "line": line,
                        "matches": match_data
                    });

                    let _ = ws_tx.send(event.to_string());
                }
            }
        });
    }
}
