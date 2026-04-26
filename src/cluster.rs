// src/cluster.rs
//
// Multi-node cluster support.
//
// Agent mode:
//   Listens to the internal broadcast channel (ws_rx) and forwards all JSON
//   event payloads via HTTP POST to the central Hub server.
//
// Server (Hub) mode:
//   Exposes an HTTP endpoint `/api/v1/ingest` to receive JSON payloads from agents.
//   It processes these payloads (updates Prometheus, inserts to SQLite, and
//   bounces to its own WebSocket clients) to provide a unified dashboard.

use anyhow::Result;
use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use reqwest::Client;
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::storage::Storage;

// ──────────────────────────────────────────────────────────────────────────────
// Agent Mode: Forward events to Hub
// ──────────────────────────────────────────────────────────────────────────────

pub async fn run_agent(cfg: Arc<Config>, mut rx: broadcast::Receiver<String>) {
    let url = format!(
        "{}/api/v1/ingest",
        cfg.cluster.server_url.trim_end_matches('/')
    );
    let secret = cfg.cluster.shared_secret.clone();

    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap_or_default();

    info!("Cluster Agent running. Forwarding metrics to {}", url);

    loop {
        match rx.recv().await {
            Ok(payload) => {
                let req = client
                    .post(&url)
                    .header("Authorization", format!("Bearer {}", secret))
                    .header("Content-Type", "application/json")
                    .body(payload);

                match req.send().await {
                    Ok(resp) => {
                        if !resp.status().is_success() {
                            warn!("Hub rejected payload: {}", resp.status());
                        } else {
                            debug!("Forwarded event to Hub successfully");
                        }
                    }
                    Err(e) => warn!("Failed to send data to Hub at {}: {}", url, e),
                }
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                warn!("Agent event queue lagged by {} messages", n);
            }
            Err(broadcast::error::RecvError::Closed) => {
                error!("Agent event channel closed. Exiting agent loop.");
                break;
            }
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Hub Mode: Ingest endpoint
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct HubState {
    pub ws_tx: broadcast::Sender<String>,
    #[allow(dead_code)]
    pub storage: Arc<Storage>,
    pub secret: String,
}

pub fn hub_router(state: HubState) -> Router {
    Router::new()
        .route("/api/v1/ingest", post(ingest_handler))
        .with_state(state)
}

async fn ingest_handler(
    State(state): State<HubState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<serde_json::Value>,
) -> Result<StatusCode, StatusCode> {
    // 1. Authenticate
    if !state.secret.is_empty() {
        let auth_hdr = headers.get("Authorization").and_then(|h| h.to_str().ok());
        let expected = format!("Bearer {}", state.secret);
        if auth_hdr != Some(expected.as_str()) {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }

    let event_str = payload.to_string();

    // 2. Broadcast to Hub's WebSocket clients
    let _ = state.ws_tx.send(event_str.clone());

    // 3. To Do: Map the raw JSON payload to update Hub's SQLite DB + Prometheus metrics.
    // For now, this is adequate to render real-time graphs on the central monitor UI.

    Ok(StatusCode::OK)
}
