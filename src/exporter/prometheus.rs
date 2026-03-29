// src/exporter/prometheus.rs
//
// Exposes all collected metrics in Prometheus text format at GET /metrics
// Also exposes a JSON API at GET /api/status for dashboards.

use axum::{
    extract::{State, ws::{WebSocket, WebSocketUpgrade, Message}, Request},
    http::{header, StatusCode, Method, HeaderValue},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router, Json,
};
use serde::Deserialize;
use base64::prelude::*;
use rust_embed::RustEmbed;
use mime_guess::from_path;
use prometheus::{
    register_gauge_vec, register_counter_vec,
    GaugeVec, CounterVec, TextEncoder, Encoder,
};
use subtle::ConstantTimeEq;
use once_cell::sync::Lazy;
use tokio::sync::broadcast;
use tracing::info;

use crate::proxmox_api::{GuestKind, GuestStatus, NodeStatus, StorageStatus};
use crate::collectors::lxc::LxcDetailedStats;

#[derive(RustEmbed)]
#[folder = "frontend/build/"]
struct Assets;

// ──────────────────────────────────────────────────────────────────────────────
// Metric definitions
// ──────────────────────────────────────────────────────────────────────────────

static NODE_CPU_USAGE: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_cpu_usage_ratio",
        "CPU usage ratio (0.0-1.0) of a Proxmox node",
        &["node"]
    ).unwrap()
});

static NODE_MEM_USED: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_memory_used_bytes",
        "Memory used in bytes on a Proxmox node",
        &["node"]
    ).unwrap()
});

static NODE_MEM_TOTAL: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_memory_total_bytes",
        "Total memory in bytes on a Proxmox node",
        &["node"]
    ).unwrap()
});

static NODE_DISK_USED: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_rootfs_used_bytes",
        "Root filesystem used bytes on a Proxmox node",
        &["node"]
    ).unwrap()
});

static NODE_DISK_TOTAL: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_rootfs_total_bytes",
        "Root filesystem total bytes on a Proxmox node",
        &["node"]
    ).unwrap()
});

static NODE_LOAD_AVG: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_load_average",
        "Load average on a Proxmox node",
        &["node", "interval"]
    ).unwrap()
});

static NODE_UPTIME: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_uptime_seconds",
        "Uptime in seconds",
        &["node"]
    ).unwrap()
});

// Guest (VM/LXC) metrics
static GUEST_CPU: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_cpu_usage_ratio",
        "CPU usage ratio for a VM or LXC",
        &["vmid", "name", "node", "type", "status"]
    ).unwrap()
});

static GUEST_MEM_USED: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_memory_used_bytes",
        "Memory used by a VM or LXC",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

static GUEST_MEM_TOTAL: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_memory_total_bytes",
        "Memory configured for a VM or LXC",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

static GUEST_NET_IN: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_network_in_bytes_total",
        "Total bytes received by a VM or LXC",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

static GUEST_NET_OUT: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_network_out_bytes_total",
        "Total bytes sent by a VM or LXC",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

static GUEST_DISK_READ: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_disk_read_bytes_total",
        "Cumulative disk read bytes",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

static OOM_KILL_TOTAL: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "pve_oom_kill_total",
        "Total number of OOM Killer events detected",
        &["node"]
    ).unwrap()
});

static GUEST_DISK_WRITE: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_disk_write_bytes_total",
        "Cumulative disk write bytes",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

static GUEST_UPTIME: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_uptime_seconds",
        "Uptime of a running guest",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

static GUEST_STATUS: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_guest_running",
        "1 if guest is running, 0 otherwise",
        &["vmid", "name", "node", "type"]
    ).unwrap()
});

// LXC cgroup detail
static LXC_MEM_CURRENT: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_lxc_cgroup_memory_current_bytes",
        "Current memory usage from cgroup v2",
        &["vmid", "name"]
    ).unwrap()
});

static LXC_MEM_ANON: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_lxc_cgroup_memory_anon_bytes",
        "Anonymous (heap/stack) memory from cgroup",
        &["vmid", "name"]
    ).unwrap()
});

static LXC_CPU_THROTTLED: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_lxc_cgroup_cpu_throttled_total",
        "Total CPU throttle events from cgroup",
        &["vmid", "name"]
    ).unwrap()
});

static LXC_PID_COUNT: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_lxc_cgroup_pid_count",
        "Current PID count from cgroup",
        &["vmid", "name"]
    ).unwrap()
});

static LXC_SWAP_CURRENT: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_lxc_cgroup_swap_current_bytes",
        "Current swap usage from cgroup",
        &["vmid", "name"]
    ).unwrap()
});

static NODE_SWAP_USED: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_swap_used_bytes",
        "Node swap used bytes",
        &["node"]
    ).unwrap()
});

static NODE_SWAP_TOTAL: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_node_swap_total_bytes",
        "Node swap total bytes",
        &["node"]
    ).unwrap()
});

// Storage
static STORAGE_USED: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_storage_used_bytes",
        "Storage used bytes",
        &["storage", "node", "type"]
    ).unwrap()
});

static STORAGE_TOTAL: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_storage_total_bytes",
        "Storage total bytes",
        &["storage", "node", "type"]
    ).unwrap()
});

static STORAGE_AVAIL: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_storage_avail_bytes",
        "Storage available bytes",
        &["storage", "node", "type"]
    ).unwrap()
});

// Log alerts
#[allow(dead_code)]
static LOG_ALERTS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "pve_log_alert_total",
        "Total log alerts matched",
        &["source", "pattern", "severity"]
    ).unwrap()
});

// HAProxy metrics
static HAPROXY_SERVER_UP: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "haproxy_server_up",
        "HAProxy server status (1=UP, 0=DOWN)",
        &["proxy", "server"]
    ).unwrap()
});

static HAPROXY_SESSIONS: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "haproxy_server_sessions_current",
        "HAProxy current sessions",
        &["proxy", "server"]
    ).unwrap()
});

static HAPROXY_BYTES_IN: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "haproxy_server_bytes_in_total",
        "HAProxy bytes received",
        &["proxy", "server"]
    ).unwrap()
});

static HAPROXY_BYTES_OUT: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "haproxy_server_bytes_out_total",
        "HAProxy bytes sent",
        &["proxy", "server"]
    ).unwrap()
});

static HAPROXY_HTTP_5XX: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "haproxy_server_http_5xx_total",
        "HAProxy HTTP 5xx responses",
        &["proxy", "server"]
    ).unwrap()
});

static HAPROXY_DOWNTIME: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "haproxy_server_downtime_seconds",
        "HAProxy server total downtime",
        &["proxy", "server"]
    ).unwrap()
});

// Database & Storage metrics
static POSTGRES_UP: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_postgres_up",
        "Postgres connection status (1 = up, 0 = down)",
        &["name"]
    ).unwrap()
});

static POSTGRES_CONNECTIONS: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_postgres_connections_total",
        "Total number of active connections",
        &["name"]
    ).unwrap()
});

static POSTGRES_LATENCY_MS: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_postgres_avg_query_latency_ms",
        "Average query latency in milliseconds",
        &["name"]
    ).unwrap()
});

static REDIS_UP: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_redis_up",
        "Redis connection status (1 = up, 0 = down)",
        &["name"]
    ).unwrap()
});

static REDIS_MEMORY: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_redis_memory_used_bytes",
        "Redis memory usage in bytes",
        &["name"]
    ).unwrap()
});

static OBJECT_STORAGE_UP: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_object_storage_up",
        "Object storage health (1 = healthy, 0 = down)",
        &["name"]
    ).unwrap()
});

static OBJECT_STORAGE_LATENCY_MS: Lazy<GaugeVec> = Lazy::new(|| {
    register_gauge_vec!(
        "pve_object_storage_latency_ms",
        "Object storage request latency in ms",
        &["name"]
    ).unwrap()
});

// ──────────────────────────────────────────────────────────────────────────────
// Metric update functions
// ──────────────────────────────────────────────────────────────────────────────

pub fn inc_oom_killer(node: &str) {
    OOM_KILL_TOTAL.with_label_values(&[node]).inc();
}

pub fn update_postgres(name: &str, up: bool, conns: i64, latency_ms: f64) {
    let status = if up { 1.0 } else { 0.0 };
    POSTGRES_UP.with_label_values(&[name]).set(status);
    POSTGRES_CONNECTIONS.with_label_values(&[name]).set(conns as f64);
    POSTGRES_LATENCY_MS.with_label_values(&[name]).set(latency_ms);
}

pub fn update_redis(name: &str, up: bool, mem_bytes: i64) {
    let status = if up { 1.0 } else { 0.0 };
    REDIS_UP.with_label_values(&[name]).set(status);
    REDIS_MEMORY.with_label_values(&[name]).set(mem_bytes as f64);
}

pub fn update_object_storage(name: &str, up: bool, latency_ms: f64) {
    let status = if up { 1.0 } else { 0.0 };
    OBJECT_STORAGE_UP.with_label_values(&[name]).set(status);
    OBJECT_STORAGE_LATENCY_MS.with_label_values(&[name]).set(latency_ms);
}

pub fn update_node(n: &NodeStatus) {
    let nd = &n.node;
    NODE_CPU_USAGE.with_label_values(&[nd]).set(n.cpu_usage);
    NODE_MEM_USED.with_label_values(&[nd]).set(n.mem_used as f64);
    NODE_MEM_TOTAL.with_label_values(&[nd]).set(n.mem_total as f64);
    NODE_SWAP_USED.with_label_values(&[nd]).set(n.swap_used as f64);
    NODE_SWAP_TOTAL.with_label_values(&[nd]).set(n.swap_total as f64);
    NODE_DISK_USED.with_label_values(&[nd]).set(n.disk_used as f64);
    NODE_DISK_TOTAL.with_label_values(&[nd]).set(n.disk_total as f64);
    NODE_LOAD_AVG.with_label_values(&[nd, "1"]).set(n.load_avg1);
    NODE_LOAD_AVG.with_label_values(&[nd, "5"]).set(n.load_avg5);
    NODE_LOAD_AVG.with_label_values(&[nd, "15"]).set(n.load_avg15);
    NODE_UPTIME.with_label_values(&[nd]).set(n.uptime as f64);
}

pub fn update_guest(g: &GuestStatus) {
    let vmid = g.vmid.to_string();
    let kind = match g.kind {
        GuestKind::Vm => "vm",
        GuestKind::Lxc => "lxc",
    };
    let labels = &[vmid.as_str(), g.name.as_str(), g.node.as_str(), kind];
    let labels_status = &[vmid.as_str(), g.name.as_str(), g.node.as_str(), kind, g.status.as_str()];

    GUEST_CPU.with_label_values(labels_status).set(g.cpu_usage);
    GUEST_MEM_USED.with_label_values(labels).set(g.mem_used as f64);
    GUEST_MEM_TOTAL.with_label_values(labels).set(g.mem_total as f64);
    GUEST_NET_IN.with_label_values(labels).set(g.net_in as f64);
    GUEST_NET_OUT.with_label_values(labels).set(g.net_out as f64);
    GUEST_DISK_READ.with_label_values(labels).set(g.disk_read as f64);
    GUEST_DISK_WRITE.with_label_values(labels).set(g.disk_write as f64);
    GUEST_UPTIME.with_label_values(labels).set(g.uptime as f64);
    GUEST_STATUS.with_label_values(labels).set(if g.status == "running" { 1.0 } else { 0.0 });
}

pub fn update_lxc_detail(s: &LxcDetailedStats) {
    let vmid = s.vmid.to_string();
    let name = s.name.as_str();
    let cg = &s.cgroup;

    LXC_MEM_CURRENT.with_label_values(&[&vmid, name]).set(cg.mem_current as f64);
    LXC_MEM_ANON.with_label_values(&[&vmid, name]).set(cg.mem_anon as f64);
    LXC_CPU_THROTTLED.with_label_values(&[&vmid, name]).set(cg.cpu_nr_throttled as f64);
    LXC_PID_COUNT.with_label_values(&[&vmid, name]).set(cg.pid_current as f64);
    LXC_SWAP_CURRENT.with_label_values(&[&vmid, name]).set(cg.mem_swap_current as f64);
}

pub fn update_storage(s: &StorageStatus) {
    let labels = &[s.storage.as_str(), s.node.as_str(), s.kind.as_str()];
    STORAGE_USED.with_label_values(labels).set(s.used as f64);
    STORAGE_TOTAL.with_label_values(labels).set(s.total as f64);
    STORAGE_AVAIL.with_label_values(labels).set(s.avail as f64);
}

#[allow(dead_code)]
pub fn record_log_alert(source: &str, pattern: &str, severity: &str) {
    LOG_ALERTS.with_label_values(&[source, pattern, severity]).inc();
}

pub fn update_haproxy(stats: &crate::collectors::haproxy::HaproxyStats) {
    for proxy in &stats.proxies {
        for server in &proxy.servers {
            let labels = &[proxy.name.as_str(), server.server_name.as_str()];
            let up_val = if server.status == "UP" { 1.0 } else { 0.0 };

            HAPROXY_SERVER_UP.with_label_values(labels).set(up_val);
            HAPROXY_SESSIONS.with_label_values(labels).set(server.sessions_current as f64);
            HAPROXY_BYTES_IN.with_label_values(labels).set(server.bytes_in as f64);
            HAPROXY_BYTES_OUT.with_label_values(labels).set(server.bytes_out as f64);
            HAPROXY_HTTP_5XX.with_label_values(labels).set(server.http_5xx as f64);
            HAPROXY_DOWNTIME.with_label_values(labels).set(server.downtime_secs as f64);
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// HTTP server
// ──────────────────────────────────────────────────────────────────────────────

pub struct MetricsServer {
    pub addr: String,
    pub tx: broadcast::Sender<String>,
    pub hub_state: Option<crate::cluster::HubState>,
    pub auth: Option<String>,
    pub storage: Option<std::sync::Arc<crate::storage::Storage>>,
}

#[derive(Clone)]
struct AppState {
    tx: broadcast::Sender<String>,
    expected_auth: Option<String>, // "Basic <base64>"
    storage: Option<std::sync::Arc<crate::storage::Storage>>,
}

impl MetricsServer {
    pub fn new(addr: &str, port: u16, tx: broadcast::Sender<String>, storage: Option<std::sync::Arc<crate::storage::Storage>>, hub_state: Option<crate::cluster::HubState>, auth: Option<String>) -> Self {
        Self {
            addr: format!("{}:{}", addr, port),
            tx,
            hub_state,
            auth,
            storage,
        }
    }

    pub async fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let expected_auth = self.auth.map(|a| format!("Basic {}", BASE64_STANDARD.encode(a)));
        let state = AppState { tx: self.tx, expected_auth, storage: self.storage };

        let mut app = Router::new()
            .route("/metrics", get(metrics_handler))
            .route("/health", get(health_handler))
            .route("/api/status", get(status_handler))
            .route("/api/v1/alerts/test", post(test_alert_handler))
            .route("/api/v1/alerts/recent", get(recent_alerts_handler))
            .route("/api/v1/history/node/:node/metrics", get(node_history_handler))
            .route("/ws", get(ws_handler))
            .fallback(static_handler)
            .route_layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
            .with_state(state);

        if let Some(hub_state) = self.hub_state {
            app = app.merge(crate::cluster::hub_router(hub_state));
            info!("Hub Ingest API → http://{}/api/v1/ingest", self.addr);
        }

        info!("Prometheus metrics → http://{}/metrics", self.addr);

        let listener = tokio::net::TcpListener::bind(&self.addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    }
}

async fn auth_middleware(
    State(state): State<AppState>,
    req: Request,
    next: Next,
) -> Result<Response, Response> {
    if let Some(ref expected) = state.expected_auth {
        let auth_header = req.headers().get(header::AUTHORIZATION).and_then(|h| h.to_str().ok()).unwrap_or("");
        
        // Use timing-safe compare to prevent timing attacks
        let is_valid = bool::from(auth_header.as_bytes().ct_eq(expected.as_bytes()));
        if !is_valid {
            let mut res = (StatusCode::UNAUTHORIZED, "Unauthorized").into_response();
            res.headers_mut().insert(
                header::WWW_AUTHENTICATE,
                HeaderValue::from_static("Basic realm=\"Proxmox Sentinel\""),
            );
            return Err(res);
        }
    }
    Ok(next.run(req).await)
}

async fn metrics_handler() -> impl IntoResponse {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap_or_default();

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        buffer,
    )
}

async fn health_handler() -> impl IntoResponse {
    (StatusCode::OK, "OK")
}

async fn recent_alerts_handler(State(state): State<AppState>) -> impl IntoResponse {
    if let Some(ref storage) = state.storage {
        match storage.query_recent_alerts(50) {
            Ok(alerts) => (StatusCode::OK, Json(serde_json::json!(alerts))).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "No storage configured").into_response()
    }
}

async fn node_history_handler(
    State(state): State<AppState>,
    axum::extract::Path(node): axum::extract::Path<String>,
) -> impl IntoResponse {
    if let Some(ref storage) = state.storage {
        match storage.query_node_history(&node, 60 * 24) { // Get last 24h by default, or could take query params
            Ok(history) => (StatusCode::OK, Json(serde_json::json!(history))).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        }
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "No storage configured").into_response()
    }
}

async fn status_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"running"}"#,
    )
}

async fn static_handler(method: Method, uri: axum::http::Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    if path.is_empty() {
        return serve_asset("index.html");
    }

    match Assets::get(path) {
        Some(_) => serve_asset(path),
        None => {
            // Support SPA routing - serve index.html for 404s
            if method == Method::GET && !path.contains('.') {
                serve_asset("index.html")
            } else {
                (StatusCode::NOT_FOUND, "Not Found").into_response()
            }
        }
    }
}

fn serve_asset(path: &str) -> Response {
    match Assets::get(path) {
        Some(content) => {
            let mime = from_path(path).first_or_octet_stream();
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref())],
                axum::body::Body::from(content.data),
            ).into_response()
        }
        None => (StatusCode::NOT_FOUND, "Not Found").into_response(),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    use futures::{stream::StreamExt, SinkExt};
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.tx.subscribe();

    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if sender.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("WS client lagged, dropped {n} messages");
                    // continue — don't disconnect slow clients
                }
                Err(_) => break, // channel closed
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Close(_) = msg {
                break;
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    }
}

#[derive(Deserialize)]
struct TestAlertRequest {
    webhook_url: String,
}

async fn test_alert_handler(
    Json(payload): Json<TestAlertRequest>,
) -> impl IntoResponse {
    let mut dispatcher = crate::alerts::AlertDispatcher::new(
        crate::config::AlertConfig {
            enabled: true,
            webhook_url: Some(payload.webhook_url),
            cpu_threshold: 100.0,
            memory_threshold: 100.0,
            disk_threshold: 100.0,
        },
        None,
    );

    dispatcher.dispatch(crate::alerts::Alert::Test {
        message: "Manually triggered from Sentinel UI".to_string(),
    }).await;

    (StatusCode::OK, "Test alert sent")
}
