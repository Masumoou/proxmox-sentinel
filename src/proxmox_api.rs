// src/proxmox_api.rs
//
// Talks to the Proxmox REST API.
// Docs: https://pve.proxmox.com/pve-docs/api-viewer/
//
use anyhow::{Context, Result};
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::time::Duration;
use tracing::{debug, warn};

use crate::config::ProxmoxConfig;

// ──────────────────────────────────────────────────────────────────────────────
// API response wrappers
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
struct ApiResponse<T> {
    data: T,
}

// ──────────────────────────────────────────────────────────────────────────────
// Domain types returned by the collector
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct NodeStatus {
    pub node: String,
    pub status: String, // "online" | "offline"
    pub cpu_usage: f64, // 0.0 – 1.0
    pub cpu_count: u32,
    pub mem_used: u64,   // bytes
    pub mem_total: u64,  // bytes
    pub swap_used: u64,  // bytes
    pub swap_total: u64, // bytes
    pub disk_used: u64,  // bytes (root fs)
    pub disk_total: u64, // bytes
    pub load_avg1: f64,
    pub load_avg5: f64,
    pub load_avg15: f64,
    pub uptime: u64, // seconds
    pub kernel_version: String,
    pub pve_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuestStatus {
    pub vmid: u32,
    pub name: String,
    pub kind: GuestKind, // VM or LXC
    pub status: String,  // "running" | "stopped" | "paused"
    pub cpu_usage: f64,  // 0.0 – 1.0
    pub cpu_count: u32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_read: u64, // bytes/s (cumulative from API)
    pub disk_write: u64,
    pub net_in: u64,
    pub net_out: u64,
    pub uptime: u64,
    pub node: String,
    pub ip_address: Option<String>,
    pub tags: Vec<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub template: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub enum GuestKind {
    Vm,
    Lxc,
}

#[derive(Debug, Clone, Serialize)]
pub struct StorageStatus {
    pub storage: String,
    pub node: String,
    pub content: String,
    pub used: u64,
    pub total: u64,
    pub avail: u64,
    pub active: bool,
    pub enabled: bool,
    pub kind: String, // "dir" | "zfspool" | "lvm" | "nfs" etc.
}

#[derive(Debug, Clone)]
pub struct GuestAgentOsInfo {
    pub name: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GuestAgentFsInfo {
    pub mountpoint: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    pub use_pct: f64,
    pub fstype: String,
}

#[derive(Debug, Clone, Serialize)]
#[allow(dead_code)]
pub struct BackupJob {
    pub id: String,
    pub node: String,
    pub start_time: i64,
    pub end_time: Option<i64>,
    pub status: String,
    pub vmid: String,
}

// ──────────────────────────────────────────────────────────────────────────────
// Raw Proxmox API shapes
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct RawNode {
    node: String,
    status: String,
    cpu: Option<f64>,
    maxcpu: Option<u32>,
    mem: Option<u64>,
    maxmem: Option<u64>,
    disk: Option<u64>,
    maxdisk: Option<u64>,
    uptime: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct RawNodeStatus {
    cpu: f64,
    #[serde(rename = "cpuinfo")]
    cpu_info: Option<RawCpuInfo>,
    memory: RawMemory,
    swap: Option<RawSwap>,
    #[serde(rename = "rootfs")]
    rootfs: Option<RawDisk>,
    loadavg: Option<Vec<serde_json::Value>>,
    uptime: u64,
    #[serde(rename = "pveversion")]
    pve_version: Option<String>,
    #[serde(rename = "uname")]
    uname: Option<serde_json::Value>,
}

#[derive(Deserialize, Debug)]
struct RawCpuInfo {
    cpus: u32,
}

#[derive(Deserialize, Debug)]
struct RawMemory {
    used: u64,
    total: u64,
}

#[derive(Deserialize, Debug)]
struct RawDisk {
    used: u64,
    total: u64,
}

#[derive(Deserialize, Debug)]
struct RawSwap {
    used: Option<u64>,
    total: Option<u64>,
}

#[derive(Deserialize, Debug)]
struct RawGuest {
    vmid: u32,
    name: Option<String>,
    status: String,
    cpu: Option<f64>,
    maxcpu: Option<u32>,
    mem: Option<u64>,
    maxmem: Option<u64>,
    diskread: Option<u64>,
    diskwrite: Option<u64>,
    netin: Option<u64>,
    netout: Option<u64>,
    uptime: Option<u64>,
    tags: Option<String>,
    template: Option<u8>,
}

#[derive(Deserialize, Debug)]
struct RawStorage {
    storage: String,
    content: Option<String>,
    used: Option<u64>,
    total: Option<u64>,
    avail: Option<u64>,
    active: Option<u8>,
    enabled: Option<u8>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

// ──────────────────────────────────────────────────────────────────────────────
// Client
// ──────────────────────────────────────────────────────────────────────────────

pub struct ProxmoxClient {
    http: Client,
    base: String,
    auth_header: String,
}

impl ProxmoxClient {
    pub fn new(cfg: &ProxmoxConfig) -> Result<Self> {
        let mut builder = ClientBuilder::new()
            .timeout(Duration::from_secs(15))
            .connection_verbose(false)
            .tcp_keepalive(Duration::from_secs(30));

        if cfg.insecure_tls {
            builder = builder.danger_accept_invalid_certs(true);
        }

        let http = builder.build().context("Building HTTP client")?;
        let auth_header = format!("PVEAPIToken={}={}", cfg.api_token_id, cfg.api_token_secret);

        Ok(Self {
            http,
            base: cfg.api_url.trim_end_matches('/').to_string(),
            auth_header,
        })
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        self.get_json(path).await
    }

    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}/api2/json{}", self.base, path);
        debug!("GET {}", url);

        let resp = self
            .http
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("API GET {} returned {}: {}", url, status, body);
        }

        let body: ApiResponse<T> = resp
            .json()
            .await
            .with_context(|| format!("Parsing {url}"))?;
        Ok(body.data)
    }

    async fn post_json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T> {
        let url = format!("{}/api2/json{}", self.base, path);
        debug!("POST {}", url);

        let resp = self
            .http
            .post(&url)
            .header("Authorization", &self.auth_header)
            .json(body)
            .send()
            .await
            .with_context(|| format!("POST {url}"))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("API POST {} returned {}: {}", url, status, body);
        }

        let body: ApiResponse<T> = resp
            .json()
            .await
            .with_context(|| format!("Parsing {url}"))?;
        Ok(body.data)
    }

    // ── Nodes ──────────────────────────────────────────────────────────────────

    pub async fn list_nodes(&self) -> Result<Vec<String>> {
        let raw: Vec<RawNode> = self.get("/nodes").await?;
        Ok(raw.into_iter().map(|n| n.node).collect())
    }

    pub async fn node_status(&self, node: &str) -> Result<NodeStatus> {
        let raw: RawNodeStatus = self.get(&format!("/nodes/{node}/status")).await?;

        let load = raw.loadavg.as_deref().unwrap_or(&[]);
        let parse_load = |v: &serde_json::Value| {
            v.as_str()
                .and_then(|s| s.parse::<f64>().ok())
                .or_else(|| v.as_f64())
                .unwrap_or(0.0)
        };

        Ok(NodeStatus {
            node: node.to_string(),
            status: "online".to_string(),
            cpu_usage: raw.cpu,
            cpu_count: raw.cpu_info.map(|c| c.cpus).unwrap_or(1),
            mem_used: raw.memory.used,
            mem_total: raw.memory.total,
            swap_used: raw.swap.as_ref().and_then(|s| s.used).unwrap_or(0),
            swap_total: raw.swap.as_ref().and_then(|s| s.total).unwrap_or(0),
            disk_used: raw.rootfs.as_ref().map(|d| d.used).unwrap_or(0),
            disk_total: raw.rootfs.as_ref().map(|d| d.total).unwrap_or(0),
            load_avg1: load.first().map(parse_load).unwrap_or(0.0),
            load_avg5: load.get(1).map(parse_load).unwrap_or(0.0),
            load_avg15: load.get(2).map(parse_load).unwrap_or(0.0),
            uptime: raw.uptime,
            kernel_version: raw
                .uname
                .as_ref()
                .and_then(|u| u.get("sysname"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            pve_version: raw.pve_version.unwrap_or_default(),
        })
    }

    // ── Guests (VMs + LXCs) ───────────────────────────────────────────────────

    pub async fn list_guests(&self, node: &str) -> Result<Vec<GuestStatus>> {
        let mut guests = Vec::new();

        // KVM VMs
        match self
            .get::<Vec<RawGuest>>(&format!("/nodes/{node}/qemu"))
            .await
        {
            Ok(vms) => {
                for vm in vms {
                    guests.push(raw_to_guest(vm, GuestKind::Vm, node));
                }
            }
            Err(e) => warn!("Failed to list VMs on {node}: {e}"),
        }

        // LXC Containers
        match self
            .get::<Vec<RawGuest>>(&format!("/nodes/{node}/lxc"))
            .await
        {
            Ok(lxcs) => {
                for lxc in lxcs {
                    guests.push(raw_to_guest(lxc, GuestKind::Lxc, node));
                }
            }
            Err(e) => warn!("Failed to list LXCs on {node}: {e}"),
        }

        Ok(guests)
    }

    /// Get detailed current status for a single VM
    #[allow(dead_code)]
    pub async fn vm_status(&self, node: &str, vmid: u32) -> Result<GuestStatus> {
        let raw: RawGuest = self
            .get(&format!("/nodes/{node}/qemu/{vmid}/status/current"))
            .await?;
        Ok(raw_to_guest(raw, GuestKind::Vm, node))
    }

    /// Get detailed current status for a single LXC
    #[allow(dead_code)]
    pub async fn lxc_status(&self, node: &str, vmid: u32) -> Result<GuestStatus> {
        let raw: RawGuest = self
            .get(&format!("/nodes/{node}/lxc/{vmid}/status/current"))
            .await?;
        Ok(raw_to_guest(raw, GuestKind::Lxc, node))
    }

    /// Get IP address from QEMU guest agent
    pub async fn vm_agent_ip(&self, node: &str, vmid: u32) -> Option<String> {
        #[derive(Deserialize)]
        struct NetworkInterfaces {
            result: Vec<NetworkIface>,
        }
        #[derive(Deserialize)]
        struct NetworkIface {
            name: String,
            #[serde(rename = "ip-addresses")]
            ip_addresses: Option<Vec<IpAddr>>,
        }
        #[derive(Deserialize)]
        struct IpAddr {
            #[serde(rename = "ip-address")]
            ip: String,
            #[serde(rename = "ip-address-type")]
            kind: String,
        }

        let result: Result<NetworkInterfaces> = self
            .get(&format!(
                "/nodes/{node}/qemu/{vmid}/agent/network-get-interfaces"
            ))
            .await;

        result.ok().and_then(|ni| {
            ni.result.iter().filter(|i| i.name != "lo").find_map(|i| {
                i.ip_addresses.as_ref()?.iter().find_map(|a| {
                    if a.kind == "ipv4" {
                        Some(a.ip.clone())
                    } else {
                        None
                    }
                })
            })
        })
    }

    /// Check whether QEMU guest agent responds.
    pub async fn vm_agent_ping(&self, node: &str, vmid: u32) -> Result<()> {
        let _: Value = self
            .post_json(
                &format!("/nodes/{node}/qemu/{vmid}/agent/ping"),
                &serde_json::json!({}),
            )
            .await?;
        Ok(())
    }

    pub async fn vm_agent_os_info(&self, node: &str, vmid: u32) -> Result<GuestAgentOsInfo> {
        let value: Value = self
            .get_json(&format!("/nodes/{node}/qemu/{vmid}/agent/get-osinfo"))
            .await?;
        Ok(parse_guest_agent_os_info(agent_payload(&value)))
    }

    pub async fn vm_agent_fs_info(&self, node: &str, vmid: u32) -> Result<Vec<GuestAgentFsInfo>> {
        let value: Value = self
            .get_json(&format!("/nodes/{node}/qemu/{vmid}/agent/get-fsinfo"))
            .await?;
        Ok(parse_guest_agent_fs_info(agent_payload(&value)))
    }

    /// Run a command inside a VM via QEMU guest agent
    #[allow(dead_code)]
    pub async fn vm_agent_exec(&self, node: &str, vmid: u32, cmd: &[&str]) -> Result<String> {
        self.vm_agent_exec_args(node, vmid, cmd).await
    }

    pub async fn vm_agent_exec_shell(
        &self,
        node: &str,
        vmid: u32,
        command: &str,
    ) -> Result<String> {
        self.vm_agent_exec_args(node, vmid, &["/bin/sh", "-lc", command])
            .await
    }

    async fn vm_agent_exec_args(&self, node: &str, vmid: u32, command: &[&str]) -> Result<String> {
        #[derive(Serialize)]
        struct ExecReq {
            command: Vec<String>,
        }
        let resp: Value = self
            .post_json(
                &format!("/nodes/{node}/qemu/{vmid}/agent/exec"),
                &ExecReq {
                    command: command.iter().map(|part| (*part).to_string()).collect(),
                },
            )
            .await?;
        let pid = extract_agent_pid(&resp).context("agent exec response did not include pid")?;

        // Poll until exited (max 10s)
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let status: Value = self
                .get(&format!(
                    "/nodes/{node}/qemu/{vmid}/agent/exec-status?pid={pid}"
                ))
                .await?;
            let exec_status = parse_agent_exec_status(&status);
            if exec_status.exited {
                if exec_status.exitcode.unwrap_or(0) == 0 {
                    return Ok(exec_status.out_data);
                }
                anyhow::bail!(
                    "agent-exec failed for vmid {vmid} with exitcode {:?}: stdout='{}' stderr='{}'",
                    exec_status.exitcode,
                    exec_status.out_data.trim(),
                    exec_status.err_data.trim()
                );
            }
        }
        anyhow::bail!("agent-exec timed out for vmid {vmid}")
    }

    // ── Storage ───────────────────────────────────────────────────────────────

    pub async fn storage_status(&self, node: &str) -> Result<Vec<StorageStatus>> {
        let raw: Vec<RawStorage> = self.get(&format!("/nodes/{node}/storage")).await?;
        Ok(raw
            .into_iter()
            .map(|s| StorageStatus {
                storage: s.storage,
                node: node.to_string(),
                content: s.content.unwrap_or_default(),
                used: s.used.unwrap_or(0),
                total: s.total.unwrap_or(0),
                avail: s.avail.unwrap_or(0),
                active: s.active.unwrap_or(0) == 1,
                enabled: s.enabled.unwrap_or(1) == 1,
                kind: s.kind.unwrap_or_default(),
            })
            .collect())
    }

    /// List storage content rows for a content type such as "backup" or "iso".
    pub async fn storage_content(
        &self,
        node: &str,
        storage: &str,
        content: &str,
    ) -> Result<Vec<serde_json::Value>> {
        self.get(&format!(
            "/nodes/{node}/storage/{storage}/content?content={content}"
        ))
        .await
    }

    /// List snapshots using Proxmox API metadata. This is preferred over parsing qm/pct text output.
    pub async fn guest_snapshots(
        &self,
        node: &str,
        kind: &GuestKind,
        vmid: u32,
    ) -> Result<Vec<serde_json::Value>> {
        let guest_type = match kind {
            GuestKind::Vm => "qemu",
            GuestKind::Lxc => "lxc",
        };
        self.get(&format!("/nodes/{node}/{guest_type}/{vmid}/snapshot"))
            .await
    }

    // ── Cluster ───────────────────────────────────────────────────────────────

    #[allow(dead_code)]
    pub async fn cluster_status(&self) -> Result<serde_json::Value> {
        self.get("/cluster/status").await
    }
}

fn raw_to_guest(raw: RawGuest, kind: GuestKind, node: &str) -> GuestStatus {
    let tags: Vec<String> = raw
        .tags
        .as_deref()
        .unwrap_or("")
        .split(';')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    let name = raw.name.unwrap_or_else(|| format!("{}", raw.vmid));
    let (os_name, os_version) = infer_guest_os(&name, &tags);

    GuestStatus {
        vmid: raw.vmid,
        name,
        kind,
        status: raw.status,
        cpu_usage: raw.cpu.unwrap_or(0.0),
        cpu_count: raw.maxcpu.unwrap_or(0),
        mem_used: raw.mem.unwrap_or(0),
        mem_total: raw.maxmem.unwrap_or(0),
        disk_read: raw.diskread.unwrap_or(0),
        disk_write: raw.diskwrite.unwrap_or(0),
        net_in: raw.netin.unwrap_or(0),
        net_out: raw.netout.unwrap_or(0),
        uptime: raw.uptime.unwrap_or(0),
        node: node.to_string(),
        ip_address: None, // enriched later
        tags,
        os_name,
        os_version,
        template: raw.template.unwrap_or(0) == 1,
    }
}

fn agent_payload(value: &Value) -> &Value {
    value.get("result").unwrap_or(value)
}

fn parse_guest_agent_os_info(value: &Value) -> GuestAgentOsInfo {
    let name = value
        .get("name")
        .and_then(Value::as_str)
        .or_else(|| value.get("pretty-name").and_then(Value::as_str))
        .map(str::to_string);
    let version = value
        .get("version")
        .and_then(Value::as_str)
        .or_else(|| value.get("version-id").and_then(Value::as_str))
        .map(str::to_string);

    GuestAgentOsInfo { name, version }
}

fn parse_guest_agent_fs_info(value: &Value) -> Vec<GuestAgentFsInfo> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            let mountpoint = item.get("mountpoint").and_then(Value::as_str)?.to_string();
            let total = json_u64(item, &["total-bytes", "total_bytes", "total"]);
            let used = json_u64(item, &["used-bytes", "used_bytes", "used"]);
            let avail = total.saturating_sub(used);
            Some(GuestAgentFsInfo {
                mountpoint,
                total,
                used,
                avail,
                use_pct: pct(used, total),
                fstype: item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

fn extract_agent_pid(value: &Value) -> Option<u64> {
    agent_payload(value).get("pid").and_then(Value::as_u64)
}

struct AgentExecStatus {
    exited: bool,
    exitcode: Option<i64>,
    out_data: String,
    err_data: String,
}

fn parse_agent_exec_status(value: &Value) -> AgentExecStatus {
    let payload = agent_payload(value);
    AgentExecStatus {
        exited: agent_exec_exited(payload),
        exitcode: payload.get("exitcode").and_then(Value::as_i64),
        out_data: agent_exec_output(payload, &["out-data", "out_data"]),
        err_data: agent_exec_output(payload, &["err-data", "err_data"]),
    }
}

fn agent_exec_exited(value: &Value) -> bool {
    let payload = agent_payload(value);
    payload.get("exited").is_some_and(|v| {
        v.as_bool()
            .unwrap_or_else(|| v.as_u64().is_some_and(|n| n == 1))
    })
}

fn agent_exec_output(value: &Value, keys: &[&str]) -> String {
    let payload = agent_payload(value);
    keys.iter()
        .find_map(|key| payload.get(*key).and_then(Value::as_str))
        .unwrap_or("")
        .to_string()
}

fn json_u64(value: &Value, keys: &[&str]) -> u64 {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_u64))
        .unwrap_or(0)
}

fn pct(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) * 100.0
    }
}

fn infer_guest_os(name: &str, tags: &[String]) -> (Option<String>, Option<String>) {
    let haystack = format!("{} {}", name, tags.join(" ")).to_lowercase();
    let version = |prefix: &str| extract_version_after(&haystack, prefix);

    if haystack.contains("fedora") {
        return (Some("Fedora Linux".to_string()), version("fedora"));
    }
    if haystack.contains("ubuntu") {
        return (Some("Ubuntu".to_string()), version("ubuntu"));
    }
    if haystack.contains("debian") {
        return (Some("Debian GNU/Linux".to_string()), version("debian"));
    }
    if haystack.contains("rocky") {
        return (Some("Rocky Linux".to_string()), version("rocky"));
    }
    if haystack.contains("alma") {
        return (Some("AlmaLinux".to_string()), version("alma"));
    }
    if haystack.contains("centos") {
        return (Some("CentOS".to_string()), version("centos"));
    }
    if haystack.contains("windows") || haystack.contains("win-") || haystack.starts_with("win") {
        return (
            Some("Windows".to_string()),
            version("windows").or_else(|| version("win")),
        );
    }
    if haystack.contains("arch") {
        return (Some("Arch Linux".to_string()), None);
    }

    (None, None)
}

fn extract_version_after(text: &str, marker: &str) -> Option<String> {
    let start = text.find(marker)? + marker.len();
    let rest =
        text[start..].trim_start_matches(|c: char| c == '-' || c == '_' || c.is_whitespace());
    let version: String = rest
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        agent_exec_exited, agent_payload, extract_agent_pid, parse_agent_exec_status,
        parse_guest_agent_fs_info, parse_guest_agent_os_info,
    };
    use serde_json::json;

    #[test]
    fn parses_qemu_agent_osinfo_result() {
        let payload = json!({
            "result": {
                "name": "Ubuntu",
                "pretty-name": "Ubuntu 24.04.4 LTS",
                "version": "24.04.4 LTS",
                "version-id": "24.04"
            }
        });

        let info = parse_guest_agent_os_info(agent_payload(&payload));
        assert_eq!(info.name.as_deref(), Some("Ubuntu"));
        assert_eq!(info.version.as_deref(), Some("24.04.4 LTS"));
    }

    #[test]
    fn parses_qemu_agent_fsinfo_result() {
        let payload = json!({
            "result": [
                {
                    "mountpoint": "/",
                    "type": "ext4",
                    "total-bytes": 1000,
                    "used-bytes": 250
                }
            ]
        });

        let mounts = parse_guest_agent_fs_info(agent_payload(&payload));
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].mountpoint, "/");
        assert_eq!(mounts[0].avail, 750);
        assert_eq!(mounts[0].use_pct, 25.0);
    }

    #[test]
    fn extracts_qemu_agent_exec_pid_from_result_wrapper() {
        let payload = json!({ "result": { "pid": 42 } });
        assert_eq!(extract_agent_pid(&payload), Some(42));
    }

    #[test]
    fn extracts_qemu_agent_exec_pid_from_direct_payload() {
        let payload = json!({ "pid": 1573 });
        assert_eq!(extract_agent_pid(&payload), Some(1573));
    }

    #[test]
    fn parses_qemu_agent_exec_status_result() {
        let payload = json!({
            "result": {
                "exited": true,
                "out-data": "ok"
            }
        });
        assert!(agent_exec_exited(&payload));
        let status = parse_agent_exec_status(&payload);
        assert!(status.exited);
        assert_eq!(status.exitcode, None);
        assert_eq!(status.out_data, "ok");
        assert_eq!(status.err_data, "");
    }

    #[test]
    fn parses_qemu_agent_exec_status_direct_payload() {
        let payload = json!({
            "exited": 1,
            "exitcode": 0,
            "out-data": "apache2.service loaded active running The Apache HTTP Server\n",
            "err-data": ""
        });
        let status = parse_agent_exec_status(&payload);
        assert!(status.exited);
        assert_eq!(status.exitcode, Some(0));
        assert!(status.out_data.contains("apache2.service"));
        assert_eq!(status.err_data, "");
    }

    #[test]
    fn parses_qemu_agent_exec_status_failure_payload() {
        let payload = json!({
            "exited": true,
            "exitcode": 127,
            "out-data": "",
            "err-data": "/bin/sh: systemctl: not found\n"
        });
        let status = parse_agent_exec_status(&payload);
        assert!(status.exited);
        assert_eq!(status.exitcode, Some(127));
        assert!(status.err_data.contains("systemctl"));
    }
}
