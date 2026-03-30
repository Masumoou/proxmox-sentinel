// src/config.rs
//
// Configuration file parsing and defaults.
// Loaded from TOML file at startup.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

// ──────────────────────────────────────────────────────────────────────────────
// Top-level config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub proxmox: ProxmoxConfig,
    pub metrics: MetricsConfig,
    pub logs: LogConfig,
    pub alerts: AlertConfig,
    pub ssh: SshConfig,
    pub collection: CollectionConfig,
    #[serde(default)]
    pub haproxy: Option<HaproxyConfig>,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub cluster: ClusterConfig,
    #[serde(default)]
    pub services: ServicesConfig,
    #[serde(default)]
    pub postgres: Vec<PostgresConfig>,
    #[serde(default)]
    pub redis: Vec<RedisConfig>,
    #[serde(default)]
    pub object_storage: Vec<ObjectStorageConfig>,
    #[serde(default)]
    pub intelligence: IntelligenceConfig,
    #[serde(default)]
    pub file_activity: FileActivityConfig,
    #[serde(default)]
    pub app_metrics: Vec<AppMetricsConfig>,
    #[serde(default)]
    pub app_logs: Vec<AppLogsConfig>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Reading config file: {}", path.display()))?;
        let cfg: Config = toml::from_str(&content)
            .with_context(|| format!("Parsing config file: {}", path.display()))?;
        Ok(cfg)
    }

    pub fn write_example() {
        print!("{}", include_str!("../config.toml.example"));
    }

    #[allow(dead_code)]
    pub fn validate(&self) -> Result<()> {
        if self.alerts.memory_threshold <= 0.0 || self.alerts.memory_threshold > 100.0 {
            anyhow::bail!("memory_threshold must be between 0 and 100");
        }
        if self.alerts.cpu_threshold <= 0.0 || self.alerts.cpu_threshold > 100.0 {
            anyhow::bail!("cpu_threshold must be between 0 and 100");
        }
        if self.alerts.disk_threshold <= 0.0 || self.alerts.disk_threshold > 100.0 {
            anyhow::bail!("disk_threshold must be between 0 and 100");
        }
        if self.intelligence.enabled {
            if self.intelligence.critical_mem_pct <= 0.0 || self.intelligence.critical_mem_pct > 100.0 {
                anyhow::bail!("intelligence.critical_mem_pct must be between 0 and 100");
            }
            if self.intelligence.target_free_mem_pct <= 0.0 || self.intelligence.target_free_mem_pct > 100.0 {
                anyhow::bail!("intelligence.target_free_mem_pct must be between 0 and 100");
            }
        }
        Ok(())
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Section configs
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ProxmoxConfig {
    pub api_url: String,
    pub api_token_id: String,
    pub api_token_secret: String,
    #[serde(default)]
    pub nodes: Vec<String>,
    #[serde(default)]
    pub insecure_tls: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_listen_port")]
    pub listen_port: u16,
    pub auth: Option<String>,
}

fn default_listen_addr() -> String {
    "0.0.0.0".to_string()
}

fn default_listen_port() -> u16 {
    9101
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    #[serde(default = "default_tail_lines")]
    pub tail_lines: usize,
    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,
    #[serde(default)]
    pub watch_paths: Vec<String>,
    #[serde(default)]
    pub alert_patterns: Vec<LogAlertPattern>,
}

fn default_tail_lines() -> usize {
    100
}

fn default_buffer_size() -> usize {
    10000
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogAlertPattern {
    pub name: String,
    pub pattern: String,
    #[serde(default = "default_severity")]
    pub severity: AlertSeverity,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum AlertSeverity {
    Critical,
    Warning,
    Info,
}

impl AlertSeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertSeverity::Critical => "critical",
            AlertSeverity::Warning => "warning",
            AlertSeverity::Info => "info",
        }
    }
}

fn default_severity() -> AlertSeverity {
    AlertSeverity::Warning
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlertConfig {
    #[serde(default)]
    pub enabled: bool,
    pub webhook_url: Option<String>,
    #[serde(default = "default_cpu_threshold")]
    pub cpu_threshold: f64,
    #[serde(default = "default_memory_threshold")]
    pub memory_threshold: f64,
    #[serde(default = "default_disk_threshold")]
    pub disk_threshold: f64,
}

fn default_cpu_threshold() -> f64 {
    90.0
}

fn default_memory_threshold() -> f64 {
    85.0
}

fn default_disk_threshold() -> f64 {
    90.0
}

#[derive(Debug, Clone, Deserialize)]
pub struct SshConfig {
    #[serde(default = "default_key_path")]
    pub private_key_path: String,
    #[serde(default = "default_ssh_user")]
    pub user: String,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub skip_vmids: Vec<u32>,
}

fn default_key_path() -> String {
    "/root/.ssh/id_ed25519".to_string()
}

fn default_ssh_user() -> String {
    "root".to_string()
}

fn default_timeout() -> u64 {
    10
}

#[derive(Debug, Clone, Deserialize)]
pub struct CollectionConfig {
    #[serde(default = "default_api_interval")]
    pub api_interval_secs: u64,
    #[serde(default = "default_cgroup_interval")]
    pub cgroup_interval_secs: u64,
    #[serde(default = "default_vm_interval")]
    pub vm_interval_secs: u64,
    #[serde(default = "default_service_interval")]
    pub service_check_interval_secs: u64,
}

fn default_api_interval() -> u64 {
    15
}

fn default_cgroup_interval() -> u64 {
    5
}

fn default_vm_interval() -> u64 {
    30
}

fn default_service_interval() -> u64 {
    60
}

// ──────────────────────────────────────────────────────────────────────────────
// HAProxy config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct HaproxyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_haproxy_url")]
    pub stats_url: String,
    /// Basic auth in "user:password" format
    #[serde(default)]
    pub auth: Option<String>,
    #[serde(default = "default_haproxy_interval")]
    pub interval_secs: u64,
}

fn default_haproxy_url() -> String {
    "http://127.0.0.1:8404/stats;csv".to_string()
}

fn default_haproxy_interval() -> u64 {
    10
}

// ──────────────────────────────────────────────────────────────────────────────
// Storage config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_metric_retention_days")]
    pub metric_retention_days: u32,
    #[serde(default = "default_log_retention_days")]
    pub log_retention_days: u32,
    #[serde(default = "default_alert_retention_days")]
    pub alert_retention_days: u32,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: default_db_path(),
            metric_retention_days: default_metric_retention_days(),
            log_retention_days: default_log_retention_days(),
            alert_retention_days: default_alert_retention_days(),
        }
    }
}

fn default_db_path() -> String {
    "/var/lib/proxmox-sentinel/sentinel.db".to_string()
}

fn default_metric_retention_days() -> u32 {
    7 // 1 week of high-res metrics
}

fn default_log_retention_days() -> u32 {
    14 // 2 weeks of logs
}

fn default_alert_retention_days() -> u32 {
    30 // 1 month of alerts
}

// ──────────────────────────────────────────────────────────────────────────────
// Cluster config (Hub-and-spoke multi-node)
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ClusterConfig {
    #[serde(default = "default_cluster_mode")]
    pub mode: String, // "standalone", "agent", "server"
    #[serde(default = "default_server_url")]
    pub server_url: String, // http://10.10.x.x:9101 (used by agent)
    #[serde(default)]
    pub shared_secret: String, // secret to authenticate agents -> server
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            mode: default_cluster_mode(),
            server_url: default_server_url(),
            shared_secret: "".to_string(),
        }
    }
}

fn default_cluster_mode() -> String {
    "standalone".to_string()
}

fn default_server_url() -> String {
    "http://127.0.0.1:9101".to_string()
}

// ──────────────────────────────────────────────────────────────────────────────
// Services (Guest internals) config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ServicesConfig {
    #[serde(default)]
    pub lxc: Vec<LxcServiceChecks>,
    #[serde(default)]
    #[allow(dead_code)]
    pub vm: Vec<VmServiceChecks>,
}

impl Default for ServicesConfig {
    fn default() -> Self {
        Self { lxc: vec![], vm: vec![] }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct LxcServiceChecks {
    pub vmid: u32,
    pub checks: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(dead_code)]
pub struct VmServiceChecks {
    pub ip: String,
    pub user: Option<String>,
    pub checks: Vec<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Postgres Config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct PostgresConfig {
    #[serde(default)]
    pub enabled: bool,
    pub name: String,
    #[serde(default)]
    pub url: String, // postgres://user:pass@localhost:5432/db
    #[serde(default)]
    pub interval_secs: u64,
}

impl Default for PostgresConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: "default".to_string(),
            url: "postgres://postgres:postgres@localhost:5432/postgres".to_string(),
            interval_secs: 60,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Redis Config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    #[serde(default)]
    pub enabled: bool,
    pub name: String,
    #[serde(default)]
    pub url: String, // redis://127.0.0.1:6379/
    #[serde(default)]
    pub interval_secs: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: "default".to_string(),
            url: "redis://127.0.0.1:6379/".to_string(),
            interval_secs: 60,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Object Storage Config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct ObjectStorageConfig {
    #[serde(default)]
    pub enabled: bool,
    pub name: String,
    #[serde(default)]
    pub endpoint: String,
    #[serde(default)]
    pub bucket: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub access_key: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub secret_key: String,
    #[serde(default)]
    pub interval_secs: u64,
}

impl Default for ObjectStorageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: "default".to_string(),
            endpoint: "".to_string(),
            bucket: "".to_string(),
            access_key: "".to_string(),
            secret_key: "".to_string(),
            interval_secs: 60,
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Intelligence Config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct IntelligenceConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_critical_mem_pct")]
    pub critical_mem_pct: f64,
    #[serde(default = "default_target_free_mem_pct")]
    pub target_free_mem_pct: f64,
}

impl Default for IntelligenceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            critical_mem_pct: default_critical_mem_pct(),
            target_free_mem_pct: default_target_free_mem_pct(),
        }
    }
}

fn default_critical_mem_pct() -> f64 { 95.0 }
fn default_target_free_mem_pct() -> f64 { 30.0 }

// ──────────────────────────────────────────────────────────────────────────────
// File Activity Config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct FileActivityConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub watch_paths: Vec<String>,
    #[serde(default = "default_activity_regex")]
    pub access_log_regex: String,
}

impl Default for FileActivityConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            watch_paths: vec![],
            access_log_regex: default_activity_regex(),
        }
    }
}

fn default_activity_regex() -> String {
    // Basic regex for common access log format extracting size (usually 10th group, but we can just provide a simple one, or user can override)
    // format: `remote_addr - remote_user [time_local] "request" status body_bytes_sent "http_referer" "http_user_agent"`
    r#"^(?P<ip>\S+)\s+\S+\s+(?P<user>\S+)\s+\[(?P<time>[^\]]+)\]\s+"(?P<method>\S+)\s+(?P<path>\S+)\s+(?P<protocol>[^"]+)"\s+(?P<status>\d+)\s+(?P<size>\d+)"#.to_string()
}

// ──────────────────────────────────────────────────────────────────────────────
// App Metrics Config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AppMetricsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub name: String,                    // display name, e.g. "nextcloud"
    pub kind: String,                    // "nextcloud_occ" | "http_json" | "shell_json"
    pub target_vmid: Option<u32>,        // LXC/VM vmid to exec into (for occ/shell)
    pub command: Option<String>,         // shell command to run inside the VM
    pub endpoint_url: Option<String>,    // HTTP endpoint for http_json kind
    pub json_path_mappings: Vec<AppMetricMapping>, // map JSON paths to metric names
    #[serde(default = "default_app_interval")]
    pub interval_secs: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppMetricMapping {
    pub json_path: String,    // e.g. "ocs.data.activeUsers.last5minutes"
    pub metric_name: String,  // e.g. "active_users_5min"
    pub metric_type: String,  // "gauge" | "counter" | "info"
    pub label: String,        // display label in UI
    pub unit: String,         // "users" | "files" | "bytes" | "ms" | ""
}

impl Default for AppMetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: "app".to_string(),
            kind: "nextcloud_occ".to_string(),
            target_vmid: None,
            command: None,
            endpoint_url: None,
            json_path_mappings: vec![],
            interval_secs: 60,
        }
    }
}

fn default_app_interval() -> u64 { 60 }

// ──────────────────────────────────────────────────────────────────────────────
// App Logs Config
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AppLogsConfig {
    #[serde(default)]
    pub enabled: bool,
    pub name: String,
    pub log_file_path: String,     // host path or pct exec path
    pub target_vmid: Option<u32>,  // if inside a container/VM
    pub log_format: String,        // "nextcloud_json" | "nginx_combined" | "apache_combined"
    pub slow_request_threshold_ms: u64,
}

impl Default for AppLogsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            name: "app".to_string(),
            log_file_path: "".to_string(),
            target_vmid: None,
            log_format: "nextcloud_json".to_string(),
            slow_request_threshold_ms: 1000,
        }
    }
}

