// src/collectors/object_storage.rs
//
// Periodically attempts to connect to an S3-compatible Object Storage
// and emits an alert if the connection or bucket fails.

use crate::alerts::{Alert, AlertDispatcher};
use crate::config::ObjectStorageConfig;
use std::time::Duration;
use tokio::time::interval;
use tracing::info;

pub async fn run_collector(cfg: ObjectStorageConfig, mut dispatcher: AlertDispatcher) {
    if !cfg.enabled || cfg.endpoint.is_empty() {
        return;
    }

    info!(
        "Starting Object Storage health collector for {}",
        cfg.endpoint
    );
    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .expect("HTTP client build");

    loop {
        ticker.tick().await;

        let url = if !cfg.bucket.is_empty() {
            format!("{}/{}", cfg.endpoint, cfg.bucket)
        } else {
            cfg.endpoint.clone()
        };

        let start = std::time::Instant::now();
        match client.get(&url).send().await {
            Ok(res) => {
                let status = res.status();
                let duration = start.elapsed().as_secs_f64() * 1000.0;

                if status.is_server_error() {
                    crate::exporter::prometheus::update_object_storage(&cfg.name, false, duration);
                    dispatcher
                        .dispatch(Alert::S3Degraded {
                            endpoint: cfg.endpoint.clone(),
                            bucket: cfg.bucket.clone(),
                            _error: format!("HTTP {} - Server error", status.as_u16()),
                        })
                        .await;
                } else {
                    crate::exporter::prometheus::update_object_storage(&cfg.name, true, duration);
                }
            }
            Err(e) => {
                crate::exporter::prometheus::update_object_storage(&cfg.name, false, 0.0);
                dispatcher
                    .dispatch(Alert::S3Degraded {
                        endpoint: cfg.endpoint.clone(),
                        bucket: cfg.bucket.clone(),
                        _error: e.to_string(),
                    })
                    .await;
            }
        }
    }
}
