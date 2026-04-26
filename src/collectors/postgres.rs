// src/collectors/postgres.rs
//
// Periodically attempts to connect to a Postgres database
// and emits an alert if the connection fails.

use crate::alerts::{Alert, AlertDispatcher};
use crate::config::PostgresConfig;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn};

pub async fn run_collector(cfg: PostgresConfig, mut dispatcher: AlertDispatcher) {
    if !cfg.enabled || cfg.url.is_empty() {
        return;
    }

    info!("Starting PostgreSQL health collector for {}", cfg.url);
    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));

    loop {
        ticker.tick().await;
        let start = std::time::Instant::now();
        match tokio_postgres::connect(&cfg.url, tokio_postgres::NoTls).await {
            Ok((client, connection)) => {
                tokio::spawn(async move {
                    if let Err(e) = connection.await {
                        warn!("Postgres connection error: {}", e);
                    }
                });

                // Query 1: Connection count and Avg latency
                let query = "
                    SELECT 
                        count(*)::int8 as conn_count,
                        extract(epoch from avg(now() - query_start)) * 1000.0 as avg_latency_ms
                    FROM pg_stat_activity 
                    WHERE state = 'active'
                ";

                match client.query_one(query, &[]).await {
                    Ok(row) => {
                        let conns: i64 = row.get("conn_count");
                        let _latency: Option<f64> = row.get("avg_latency_ms");

                        let duration = start.elapsed().as_secs_f64() * 1000.0;
                        crate::exporter::prometheus::update_postgres(
                            &cfg.name, true, conns, duration,
                        );
                    }
                    Err(e) => {
                        warn!("Postgres stats query error: {}", e);
                        crate::exporter::prometheus::update_postgres(&cfg.name, true, 0, 0.0);
                    }
                }
            }
            Err(e) => {
                crate::exporter::prometheus::update_postgres(&cfg.name, false, 0, 0.0);
                dispatcher
                    .dispatch(Alert::PostgresDown {
                        url: cfg.url.clone(),
                        _error: e.to_string(),
                    })
                    .await;
            }
        }
    }
}
