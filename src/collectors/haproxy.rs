// src/collectors/haproxy.rs
//
// HAProxy stats collector.
// Scrapes the HAProxy CSV stats endpoint (/stats;csv) and parses
// frontend, backend, and server status for dashboard + alerts.

use anyhow::{Context, Result};
use reqwest::Client;
use serde::Serialize;
use std::time::Duration;
use tracing::debug;

use crate::config::HaproxyConfig;

// ──────────────────────────────────────────────────────────────────────────────
// Types
// ──────────────────────────────────────────────────────────────────────────────

/// Row type from HAProxy stats CSV (column 33: type)
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyType {
    Frontend,
    Backend,
    Server,
    Listener,
}

#[derive(Debug, Clone, Serialize)]
pub struct HaproxyEntry {
    /// Proxy name (e.g. "disk_bg_backend")
    pub proxy_name: String,
    /// Server name within the proxy (e.g. "dky-sub-01"), empty for frontend/backend summary
    pub server_name: String,
    /// FRONTEND, BACKEND, or SERVER
    pub proxy_type: ProxyType,
    /// Status: "OPEN" for frontends, "UP"/"DOWN"/"MAINT"/"DRAIN" for servers/backends
    pub status: String,
    /// Current sessions
    pub sessions_current: u64,
    /// Max sessions
    pub sessions_max: u64,
    /// Session limit (0 = unlimited)
    pub sessions_limit: u64,
    /// Total sessions since start
    pub sessions_total: u64,
    /// Bytes in
    pub bytes_in: u64,
    /// Bytes out
    pub bytes_out: u64,
    /// Request rate (req/s) — frontends only
    pub request_rate: u64,
    /// HTTP 2xx responses
    pub http_2xx: u64,
    /// HTTP 4xx responses
    pub http_4xx: u64,
    /// HTTP 5xx responses
    pub http_5xx: u64,
    /// Downtime in seconds
    pub downtime_secs: u64,
    /// Last health check status ("L7OK", "L4CON", etc.)
    pub check_status: String,
    /// Weight of the server (for load balancing)
    pub weight: u64,
    /// Active/backup state: 1 = active, 0 = backup
    pub active: bool,
    /// Queue current
    pub queue_current: u64,
    /// Connection errors
    pub connection_errors: u64,
    /// Response errors
    pub response_errors: u64,
    /// Number of times UP→DOWN transitions
    pub check_downs: u64,
    /// Last status change (seconds ago)
    pub last_change_secs: u64,
}

/// Aggregated view per proxy (one frontend + its backends/servers)
#[derive(Debug, Clone, Serialize)]
pub struct HaproxyProxy {
    pub name: String,
    pub frontend: Option<HaproxyEntry>,
    pub backend_summary: Option<HaproxyEntry>,
    pub servers: Vec<HaproxyEntry>,
}

/// Full HAProxy instance stats
#[derive(Debug, Clone, Serialize)]
pub struct HaproxyStats {
    pub proxies: Vec<HaproxyProxy>,
    pub total_frontends: usize,
    pub total_backends: usize,
    pub total_servers: usize,
    pub servers_up: usize,
    pub servers_down: usize,
}

// ──────────────────────────────────────────────────────────────────────────────
// CSV column indices (HAProxy stats CSV v1.5+)
// ──────────────────────────────────────────────────────────────────────────────

// These are the column positions in HAProxy's CSV output.
// Reference: https://www.haproxy.com/documentation/haproxy-stats-api
const COL_PXNAME: usize = 0; // proxy name
const COL_SVNAME: usize = 1; // service name (FRONTEND/BACKEND/server)
const COL_QCUR: usize = 2; // current queued requests
const COL_SCUR: usize = 4; // current sessions
const COL_SMAX: usize = 5; // max sessions
const COL_SLIM: usize = 6; // session limit
const COL_STOT: usize = 7; // total sessions
const COL_BIN: usize = 8; // bytes in
const COL_BOUT: usize = 9; // bytes out
const COL_ECON: usize = 13; // connection errors
const COL_ERESP: usize = 14; // response errors
const COL_STATUS: usize = 17; // status (UP/DOWN/OPEN...)
const COL_WEIGHT: usize = 18; // server weight
const COL_ACT: usize = 19; // active (1) or backup (0)
const COL_DOWNTIME: usize = 24; // total downtime (s)
const COL_RATE: usize = 33; // session rate
const COL_HRSP_2XX: usize = 40; // HTTP 2xx responses
const COL_HRSP_4XX: usize = 42; // HTTP 4xx responses
const COL_HRSP_5XX: usize = 43; // HTTP 5xx responses
const COL_CHECK_STATUS: usize = 36; // health check status
const COL_CHECK_DOWNS: usize = 22; // number of UP->DOWN transitions
const COL_LASTCHG: usize = 23; // last status change (seconds)
const COL_TYPE: usize = 32; // 0=FE, 1=BE, 2=server, 3=listener

// ──────────────────────────────────────────────────────────────────────────────
// Collector
// ──────────────────────────────────────────────────────────────────────────────

pub struct HaproxyCollector {
    client: Client,
    stats_url: String,
}

impl HaproxyCollector {
    pub fn new(cfg: &HaproxyConfig) -> Result<Self> {
        let builder = Client::builder().timeout(Duration::from_secs(10));

        // Add basic auth via the URL or header
        if let Some(ref auth) = cfg.auth {
            // Auth is "user:password" format
            let parts: Vec<&str> = auth.splitn(2, ':').collect();
            if parts.len() == 2 {
                // We'll add auth header manually per-request
                debug!("HAProxy stats auth configured");
            }
        }

        let client = builder.build().context("Building HAProxy HTTP client")?;

        Ok(Self {
            client,
            stats_url: cfg.stats_url.clone(),
        })
    }

    /// Fetch and parse HAProxy stats CSV
    pub async fn collect(&self, cfg: &HaproxyConfig) -> Result<HaproxyStats> {
        let mut req = self.client.get(&self.stats_url);

        // Apply basic auth if configured
        if let Some(ref auth) = cfg.auth {
            let parts: Vec<&str> = auth.splitn(2, ':').collect();
            if parts.len() == 2 {
                req = req.basic_auth(parts[0], Some(parts[1]));
            }
        }

        let resp = req.send().await.context("Fetching HAProxy stats")?;
        let body = resp.text().await.context("Reading HAProxy stats body")?;

        self.parse_csv(&body)
    }

    fn parse_csv(&self, body: &str) -> Result<HaproxyStats> {
        let mut entries: Vec<HaproxyEntry> = Vec::new();

        for line in body.lines() {
            // Skip comments (header line starts with "# ")
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let cols: Vec<&str> = line.split(',').collect();
            if cols.len() < 45 {
                continue; // not enough columns
            }

            let proxy_type = match parse_u64(cols.get(COL_TYPE).copied()) {
                0 => ProxyType::Frontend,
                1 => ProxyType::Backend,
                2 => ProxyType::Server,
                3 => ProxyType::Listener,
                _ => continue,
            };

            entries.push(HaproxyEntry {
                proxy_name: cols[COL_PXNAME].to_string(),
                server_name: cols[COL_SVNAME].to_string(),
                proxy_type,
                status: cols.get(COL_STATUS).unwrap_or(&"").to_string(),
                sessions_current: parse_u64(cols.get(COL_SCUR).copied()),
                sessions_max: parse_u64(cols.get(COL_SMAX).copied()),
                sessions_limit: parse_u64(cols.get(COL_SLIM).copied()),
                sessions_total: parse_u64(cols.get(COL_STOT).copied()),
                bytes_in: parse_u64(cols.get(COL_BIN).copied()),
                bytes_out: parse_u64(cols.get(COL_BOUT).copied()),
                request_rate: parse_u64(cols.get(COL_RATE).copied()),
                http_2xx: parse_u64(cols.get(COL_HRSP_2XX).copied()),
                http_4xx: parse_u64(cols.get(COL_HRSP_4XX).copied()),
                http_5xx: parse_u64(cols.get(COL_HRSP_5XX).copied()),
                downtime_secs: parse_u64(cols.get(COL_DOWNTIME).copied()),
                check_status: cols.get(COL_CHECK_STATUS).unwrap_or(&"").to_string(),
                weight: parse_u64(cols.get(COL_WEIGHT).copied()),
                active: parse_u64(cols.get(COL_ACT).copied()) == 1,
                queue_current: parse_u64(cols.get(COL_QCUR).copied()),
                connection_errors: parse_u64(cols.get(COL_ECON).copied()),
                response_errors: parse_u64(cols.get(COL_ERESP).copied()),
                check_downs: parse_u64(cols.get(COL_CHECK_DOWNS).copied()),
                last_change_secs: parse_u64(cols.get(COL_LASTCHG).copied()),
            });
        }

        // Group by proxy name
        let mut proxy_map: std::collections::BTreeMap<String, HaproxyProxy> =
            std::collections::BTreeMap::new();

        let mut total_servers = 0usize;
        let mut servers_up = 0usize;
        let mut servers_down = 0usize;

        for entry in entries {
            let proxy = proxy_map
                .entry(entry.proxy_name.clone())
                .or_insert_with(|| HaproxyProxy {
                    name: entry.proxy_name.clone(),
                    frontend: None,
                    backend_summary: None,
                    servers: Vec::new(),
                });

            match entry.proxy_type {
                ProxyType::Frontend => {
                    proxy.frontend = Some(entry);
                }
                ProxyType::Backend => {
                    proxy.backend_summary = Some(entry);
                }
                ProxyType::Server => {
                    total_servers += 1;
                    if entry.status == "UP" {
                        servers_up += 1;
                    } else if entry.status == "DOWN" {
                        servers_down += 1;
                    }
                    proxy.servers.push(entry);
                }
                ProxyType::Listener => {
                    // Usually not needed for monitoring, skip
                }
            }
        }

        let proxies: Vec<HaproxyProxy> = proxy_map.into_values().collect();
        let total_frontends = proxies.iter().filter(|p| p.frontend.is_some()).count();
        let total_backends = proxies
            .iter()
            .filter(|p| p.backend_summary.is_some())
            .count();

        Ok(HaproxyStats {
            proxies,
            total_frontends,
            total_backends,
            total_servers,
            servers_up,
            servers_down,
        })
    }

    /// Return list of servers that are DOWN
    pub fn find_down_servers(stats: &HaproxyStats) -> Vec<(&str, &str, u64)> {
        let mut down = Vec::new();
        for proxy in &stats.proxies {
            for server in &proxy.servers {
                if server.status == "DOWN" {
                    down.push((
                        proxy.name.as_str(),
                        server.server_name.as_str(),
                        server.downtime_secs,
                    ));
                }
            }
        }
        down
    }
}

fn parse_u64(s: Option<&str>) -> u64 {
    s.and_then(|v| v.trim().parse().ok()).unwrap_or(0)
}
