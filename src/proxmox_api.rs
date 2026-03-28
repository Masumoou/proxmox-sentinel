// src/proxmox_api.rs
//
// Talks to the Proxmox REST API.
// Docs: https://pve.proxmox.com/pve-docs/api-viewer/
//
use anyhow::{Context, Result};
use reqwest::{Client, ClientBuilder};
use serde::{Deserialize, Serialize};
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
    pub status: String,          // "online" | "offline"
    pub cpu_usage: f64,          // 0.0 – 1.0
    pub cpu_count: u32,
    pub mem_used: u64,           // bytes
    pub mem_total: u64,          // bytes
    pub swap_used: u64,          // bytes
    pub swap_total: u64,         // bytes
    pub disk_used: u64,          // bytes (root fs)
    pub disk_total: u64,         // bytes
    pub load_avg1: f64,
    pub load_avg5: f64,
    pub load_avg15: f64,
    pub uptime: u64,             // seconds
    pub kernel_version: String,
    pub pve_version: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct GuestStatus {
    pub vmid: u32,
    pub name: String,
    pub kind: GuestKind,         // VM or LXC
    pub status: String,          // "running" | "stopped" | "paused"
    pub cpu_usage: f64,          // 0.0 – 1.0
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_read: u64,          // bytes/s (cumulative from API)
    pub disk_write: u64,
    pub net_in: u64,
    pub net_out: u64,
    pub uptime: u64,
    pub node: String,
    pub ip_address: Option<String>,
    pub tags: Vec<String>,
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
    pub kind: String,            // "dir" | "zfspool" | "lvm" | "nfs" etc.
}

#[derive(Debug, Clone, Serialize)]
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
    mem: Option<u64>,
    maxmem: Option<u64>,
    diskread: Option<u64>,
    diskwrite: Option<u64>,
    netin: Option<u64>,
    netout: Option<u64>,
    uptime: Option<u64>,
    tags: Option<String>,
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
        let auth_header = format!(
            "PVEAPIToken={}={}",
            cfg.api_token_id, cfg.api_token_secret
        );

        Ok(Self {
            http,
            base: cfg.api_url.trim_end_matches('/').to_string(),
            auth_header,
        })
    }

    async fn get<T: for<'de> Deserialize<'de>>(&self, path: &str) -> Result<T> {
        let url = format!("{}/api2/json{}", self.base, path);
        debug!("GET {}", url);

        let resp = self
            .http
            .get(&url)
            .header("Authorization", &self.auth_header)
            .send()
            .await
            .with_context(|| format!("GET {url}"))?;

        if !resp.status().is_success() {
            anyhow::bail!("API {} returned {}", url, resp.status());
        }

        let body: ApiResponse<T> = resp.json().await.with_context(|| format!("Parsing {url}"))?;
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
            load_avg1: load.get(0).map(parse_load).unwrap_or(0.0),
            load_avg5: load.get(1).map(parse_load).unwrap_or(0.0),
            load_avg15: load.get(2).map(parse_load).unwrap_or(0.0),
            uptime: raw.uptime,
            kernel_version: String::new(),
            pve_version: raw.pve_version.unwrap_or_default(),
        })
    }

    // ── Guests (VMs + LXCs) ───────────────────────────────────────────────────

    pub async fn list_guests(&self, node: &str) -> Result<Vec<GuestStatus>> {
        let mut guests = Vec::new();

        // KVM VMs
        match self.get::<Vec<RawGuest>>(&format!("/nodes/{node}/qemu")).await {
            Ok(vms) => {
                for vm in vms {
                    guests.push(raw_to_guest(vm, GuestKind::Vm, node));
                }
            }
            Err(e) => warn!("Failed to list VMs on {node}: {e}"),
        }

        // LXC Containers
        match self.get::<Vec<RawGuest>>(&format!("/nodes/{node}/lxc")).await {
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
    pub async fn vm_status(&self, node: &str, vmid: u32) -> Result<GuestStatus> {
        let raw: RawGuest = self
            .get(&format!("/nodes/{node}/qemu/{vmid}/status/current"))
            .await?;
        Ok(raw_to_guest(raw, GuestKind::Vm, node))
    }

    /// Get detailed current status for a single LXC
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
            .get(&format!("/nodes/{node}/qemu/{vmid}/agent/network-get-interfaces"))
            .await;

        result.ok().and_then(|ni| {
            ni.result
                .iter()
                .filter(|i| i.name != "lo")
                .find_map(|i| {
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

    /// Run a command inside a VM via QEMU guest agent
    pub async fn vm_agent_exec(&self, node: &str, vmid: u32, cmd: &[&str]) -> Result<String> {
        #[derive(Serialize)]
        struct ExecReq<'a> {
            command: &'a [&'a str],
        }
        #[derive(Deserialize)]
        struct ExecResp {
            pid: u64,
        }
        #[derive(Deserialize)]
        struct ExecStatus {
            exited: Option<u8>,
            #[serde(rename = "out-data")]
            out_data: Option<String>,
            #[serde(rename = "err-data")]
            err_data: Option<String>,
        }

        let url = format!(
            "{}/api2/json/nodes/{}/qemu/{}/agent/exec",
            self.base, node, vmid
        );
        let resp: ApiResponse<ExecResp> = self
            .http
            .post(&url)
            .header("Authorization", &self.auth_header)
            .json(&ExecReq { command: cmd })
            .send()
            .await?
            .json()
            .await?;

        let pid = resp.data.pid;

        // Poll until exited (max 10s)
        for _ in 0..20 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            let status: ApiResponse<ExecStatus> = self
                .get(&format!(
                    "/nodes/{node}/qemu/{vmid}/agent/exec-status?pid={pid}"
                ))
                .await?;
            if status.data.exited == Some(1) {
                return Ok(status.data.out_data.unwrap_or_default());
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

    // ── Cluster ───────────────────────────────────────────────────────────────

    pub async fn cluster_status(&self) -> Result<serde_json::Value> {
        self.get("/cluster/status").await
    }
}

fn raw_to_guest(raw: RawGuest, kind: GuestKind, node: &str) -> GuestStatus {
    let tags = raw
        .tags
        .as_deref()
        .unwrap_or("")
        .split(';')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();

    GuestStatus {
        vmid: raw.vmid,
        name: raw.name.unwrap_or_else(|| format!("{}", raw.vmid)),
        kind,
        status: raw.status,
        cpu_usage: raw.cpu.unwrap_or(0.0),
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
    }
}
