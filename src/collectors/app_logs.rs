// src/collectors/app_logs.rs

use crate::alerts::{Alert, AlertDispatcher};
use crate::config::{AppLogsConfig, Config};
use notify::{RecursiveMode, Watcher, RecommendedWatcher, Config as NotifyConfig, Event};
use serde_json::Value;
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use regex::Regex;

pub async fn run_collector(
    cfg: AppLogsConfig,
    full_cfg: Arc<Config>,
    mut dispatcher: AlertDispatcher,
    ws_tx: broadcast::Sender<String>,
) {
    if !cfg.enabled {
        return;
    }

    info!("Starting App Logs collector: {} ({})", cfg.name, cfg.log_file_path);
    
    let stats = Arc::new(Mutex::new(LogStats::new()));
    let stats_clone = stats.clone();
    let cfg_name = cfg.name.clone();
    let ws_tx_stats = ws_tx.clone();

    // Spawn 60s stats broadcaster
    tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let current_stats = stats_clone.lock().unwrap().get_and_reset();
            let event = serde_json::json!({
                "type": "app_log_stats",
                "app": cfg_name,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "requests_per_min": current_stats.0,
                "errors_per_min": current_stats.1,
                "auth_failures_per_min": current_stats.2
            });
            let _ = ws_tx_stats.send(event.to_string());
        }
    });

    let (tx, rx) = std::sync::mpsc::channel::<notify::Result<Event>>();
    let mut watcher = RecommendedWatcher::new(tx, NotifyConfig::default()).expect("Failed to create watcher");
    watcher.watch(Path::new(&cfg.log_file_path), RecursiveMode::NonRecursive).expect("Failed to watch log file");

    let mut last_offset = match File::open(&cfg.log_file_path) {
        Ok(f) => f.metadata().unwrap().len(),
        Err(_) => 0,
    };

    let regex = Regex::new(&full_cfg.file_activity.access_log_regex).ok();

    while let Ok(msg) = rx.recv() {
        if let Ok(event) = msg {
            if event.kind.is_modify() || event.kind.is_create() {
                if let Err(e) = process_new_lines(&cfg, &mut last_offset, &stats, &ws_tx, &regex) {
                    warn!("Failed to process log lines for {}: {}", cfg.name, e);
                }
            }
        }
    }
}

struct LogStats {
    request_ts: VecDeque<u64>,
    error_ts: VecDeque<u64>,
    auth_ts: VecDeque<u64>,
}

impl LogStats {
    fn new() -> Self {
        Self {
            request_ts: VecDeque::new(),
            error_ts: VecDeque::new(),
            auth_ts: VecDeque::new(),
        }
    }

    fn add_request(&mut self, is_error: bool, is_auth_fail: bool) {
        let now = chrono::Utc::now().timestamp() as u64;
        self.request_ts.push_back(now);
        if is_error { self.error_ts.push_back(now); }
        if is_auth_fail { self.auth_ts.push_back(now); }
    }

    fn get_and_reset(&mut self) -> (usize, usize, usize) {
        let now = chrono::Utc::now().timestamp() as u64;
        let cut_off = now.saturating_sub(60);
        
        while self.request_ts.front().map_or(false, |&t| t < cut_off) { self.request_ts.pop_front(); }
        while self.error_ts.front().map_or(false, |&t| t < cut_off) { self.error_ts.pop_front(); }
        while self.auth_ts.front().map_or(false, |&t| t < cut_off) { self.auth_ts.pop_front(); }
        
        (self.request_ts.len(), self.error_ts.len(), self.auth_ts.len())
    }
}

fn process_new_lines(
    cfg: &AppLogsConfig,
    offset: &mut u64,
    stats: &Arc<Mutex<LogStats>>,
    ws_tx: &broadcast::Sender<String>,
    regex: &Option<Regex>,
) -> anyhow::Result<()> {
    let mut file = File::open(&cfg.log_file_path)?;
    let metadata = file.metadata()?;
    
    if metadata.len() < *offset {
        *offset = 0; // Truncated
    }

    file.seek(SeekFrom::Start(*offset))?;
    let reader = BufReader::new(file);
    
    for line_res in reader.lines() {
        if let Ok(line) = line_res {
            parse_and_broadcast(cfg, &line, stats, ws_tx, regex);
        }
    }
    
    *offset = metadata.len();
    Ok(())
}

fn parse_and_broadcast(
    cfg: &AppLogsConfig,
    line: &str,
    stats: &Arc<Mutex<LogStats>>,
    ws_tx: &broadcast::Sender<String>,
    regex: &Option<Regex>,
) {
    let mut is_error = false;
    let mut is_auth_fail = false;
    let mut event_data = serde_json::Map::new();
    let mut level = 1; // info

    match cfg.log_format.as_str() {
        "nextcloud_json" => {
            if let Ok(val) = serde_json::from_str::<Value>(line) {
                let msg = val.get("message").and_then(|m| m.as_str()).unwrap_or("");
                let app = val.get("app").and_then(|a| a.as_str()).unwrap_or("");
                let l = val.get("level").and_then(|l| l.as_u64()).unwrap_or(1);
                
                level = if l >= 3 { 3 } else if l >= 2 { 2 } else { 1 };
                if level >= 2 { is_error = true; }

                // Auth failures
                if (level >= 2 && msg.contains("Login failed")) || (app == "core" && msg.contains("Invalid credentials")) {
                    is_auth_fail = true;
                }

                // Slow requests
                if msg.contains("slow query") || (app == "db" && level >= 2) {
                    // special handling or just flag as warning
                    level = 2;
                }

                event_data.insert("message".to_string(), serde_json::json!(msg));
                event_data.insert("app".to_string(), serde_json::json!(app));
                event_data.insert("remoteAddr".to_string(), val.get("remoteAddr").cloned().unwrap_or(Value::Null));
            }
        },
        "nginx_combined" | "apache_combined" => {
            if let Some(re) = regex {
                if let Some(caps) = re.captures(line) {
                    let status_str = caps.name("status").map(|m| m.as_str()).unwrap_or("200");
                    let status = status_str.parse::<u32>().unwrap_or(200);
                    
                    level = if status >= 500 { 3 } else if status >= 400 { 2 } else { 1 };
                    if level >= 2 { is_error = true; }

                    for name in re.capture_names().flatten() {
                        if let Some(m) = caps.name(name) {
                            event_data.insert(name.to_string(), serde_json::json!(m.as_str()));
                        }
                    }
                }
            }
        },
        _ => {}
    }

    stats.lock().unwrap().add_request(is_error, is_auth_fail);

    let event = serde_json::json!({
        "type": "app_log_event",
        "app": cfg.name,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "level": level,
        "line": line,
        "matches": event_data
    });
    let _ = ws_tx.send(event.to_string());
}
