// src/collectors/redis.rs
//
// Periodically attempts to connect to a Redis database
// and emits an alert if the connection fails.

use crate::alerts::{Alert, AlertDispatcher};
use crate::config::RedisConfig;
use redis::AsyncCommands;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn};

pub async fn run_collector(cfg: RedisConfig, mut dispatcher: AlertDispatcher) {
    if !cfg.enabled || cfg.url.is_empty() {
        return;
    }

    info!("Starting Redis health collector for {}", cfg.url);
    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));

    loop {
        ticker.tick().await;

        match redis::Client::open(cfg.url.clone()) {
            Ok(client) => match client.get_async_connection().await {
                Ok(mut con) => {
                    // Try to get INFO memory
                    let info: redis::RedisResult<String> = redis::cmd("INFO").arg("memory").query_async(&mut con).await;
                    match info {
                        Ok(text) => {
                            let mut mem_used = 0i64;
                            for line in text.lines() {
                                if line.starts_with("used_memory:") {
                                    if let Some(val) = line.split(':').nth(1) {
                                        mem_used = val.trim().parse().unwrap_or(0);
                                    }
                                }
                            }
                            crate::exporter::prometheus::update_redis(&cfg.name, true, mem_used);
                        }
                        Err(e) => {
                            warn!("Redis INFO command failed: {}", e);
                            crate::exporter::prometheus::update_redis(&cfg.name, true, 0);
                        }
                    }
                }
                Err(e) => {
                    crate::exporter::prometheus::update_redis(&cfg.name, false, 0);
                    dispatcher
                        .dispatch(Alert::RedisDown {
                            url: cfg.url.clone(),
                            _error: e.to_string(),
                        })
                        .await;
                }
            },
            Err(e) => {
                crate::exporter::prometheus::update_redis(&cfg.name, false, 0);
                warn!("Redis config URL invalid: {}", e);
            }
        }
    }
}
