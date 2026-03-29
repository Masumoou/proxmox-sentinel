// src/collectors/vm.rs
//
// KVM VM monitoring via:
//   1. QEMU Guest Agent  →  network interfaces, filesystem info, process list
//   2. SSH               →  service status, log tailing, disk usage
//
// The guest agent approach is preferred (no SSH key setup needed).
// SSH is the fallback for log collection.

use anyhow::{Context, Result};
use serde::Serialize;
use ssh2::Session;
use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, warn};

use crate::config::SshConfig;
use crate::proxmox_api::ProxmoxClient;

#[derive(Debug, Clone, Serialize)]
pub struct VmDetailedStats {
    pub vmid: u32,
    pub name: String,
    pub ip_address: Option<String>,
    pub services: Vec<VmService>,
    pub disk_mounts: Vec<VmDiskMount>,
    pub top_processes: Vec<VmProcess>,
    pub agent_available: bool,
    pub ssh_available: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct VmService {
    pub name: String,
    pub active: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VmDiskMount {
    pub mountpoint: String,
    pub total: u64,
    pub used: u64,
    pub avail: u64,
    pub use_pct: f64,
    pub fstype: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct VmProcess {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f64,
    pub mem_rss_kb: u64,
}

pub struct VmCollector<'a> {
    client: &'a ProxmoxClient,
    ssh_cfg: &'a SshConfig,
}

impl<'a> VmCollector<'a> {
    pub fn new(client: &'a ProxmoxClient, ssh_cfg: &'a SshConfig) -> Self {
        Self { client, ssh_cfg }
    }

    pub async fn collect(&self, node: &str, vmid: u32, name: &str) -> VmDetailedStats {
        let mut stats = VmDetailedStats {
            vmid,
            name: name.to_string(),
            ip_address: None,
            services: vec![],
            disk_mounts: vec![],
            top_processes: vec![],
            agent_available: false,
            ssh_available: false,
        };

        // Try QEMU guest agent first
        match self.collect_via_agent(node, vmid).await {
            Ok(agent_data) => {
                stats.agent_available = true;
                stats.ip_address = agent_data.ip;
                stats.disk_mounts = agent_data.mounts;
                stats.top_processes = agent_data.processes;

                // If we have an IP, also try SSH for service status and logs
                if let Some(ref ip) = stats.ip_address.clone() {
                    match self.collect_via_ssh(ip).await {
                        Ok(ssh_data) => {
                            stats.ssh_available = true;
                            stats.services = ssh_data.services;
                        }
                        Err(e) => debug!("SSH to vm {} ({ip}): {e}", vmid),
                    }
                }
            }
            Err(e) => {
                debug!("Guest agent unavailable for vm {vmid}: {e}");
                // Pure SSH fallback
                if let Some(ip) = self.client.vm_agent_ip(node, vmid).await {
                    stats.ip_address = Some(ip.clone());
                    match self.collect_via_ssh(&ip).await {
                        Ok(ssh_data) => {
                            stats.ssh_available = true;
                            stats.services = ssh_data.services;
                            stats.disk_mounts = ssh_data.mounts;
                        }
                        Err(e) => warn!("SSH fallback failed for vm {vmid}: {e}"),
                    }
                }
            }
        }

        stats
    }

    // ── Guest Agent path ───────────────────────────────────────────────────

    async fn collect_via_agent(&self, node: &str, vmid: u32) -> Result<AgentData> {
        // Get filesystem info
        let fs_json = self
            .client
            .vm_agent_exec(
                node,
                vmid,
                &["df", "--output=source,fstype,size,used,avail,pcent,target",
                  "-k", "--block-size=1", "--no-sync"],
            )
            .await?;

        let mounts = parse_df_output(&fs_json);

        // Get process list via agent exec
        let ps_json = self
            .client
            .vm_agent_exec(
                node,
                vmid,
                &["ps", "-eo", "pid,comm,pcpu,rss", "--no-headers", "--sort=-pcpu"],
            )
            .await?;

        let processes = parse_ps_output(&ps_json);

        // Get primary IP
        let ip = self.client.vm_agent_ip(node, vmid).await;

        Ok(AgentData { ip, mounts, processes })
    }

    // ── SSH path ───────────────────────────────────────────────────────────

    async fn collect_via_ssh(&self, ip: &str) -> Result<SshData> {
        let cfg = self.ssh_cfg.clone();
        let ip_owned = ip.to_string();

        // SSH is blocking; run on thread pool
        tokio::task::spawn_blocking(move || {
            ssh_collect(&ip_owned, &cfg)
        })
        .await
        .context("SSH task panicked")?
    }

    /// Tail the last N lines of a log file on a VM via SSH
    #[allow(dead_code)]
    pub async fn tail_log(&self, ip: &str, path: &str, lines: usize) -> Result<Vec<String>> {
        let cfg = self.ssh_cfg.clone();
        let ip = ip.to_string();
        let path = path.to_string();

        tokio::task::spawn_blocking(move || -> Result<Vec<String>> {
            let sess = ssh_connect(&ip, &cfg)?;
            let output = ssh_exec(&sess, &format!("tail -n {lines} {path}"))?;
            Ok(output.lines().map(str::to_string).collect())
        })
        .await
        .context("SSH tail task")?
    }

    /// Check status of specific services on a VM
    #[allow(dead_code)]
    pub async fn check_services(&self, ip: &str, service_names: &[&str]) -> Result<Vec<VmService>> {
        let cfg = self.ssh_cfg.clone();
        let ip = ip.to_string();
        let names: Vec<String> = service_names.iter().map(|s| s.to_string()).collect();

        tokio::task::spawn_blocking(move || -> Result<Vec<VmService>> {
            let sess = ssh_connect(&ip, &cfg)?;
            let mut services = Vec::new();
            for name in &names {
                let output = ssh_exec(
                    &sess,
                    &format!("systemctl is-active {} 2>/dev/null || echo unknown", name),
                )?;
                let status = output.trim().to_string();
                services.push(VmService {
                    name: name.clone(),
                    active: status == "active",
                    status,
                });
            }
            Ok(services)
        })
        .await
        .context("SSH check-services")?
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// SSH helpers (sync, run in spawn_blocking)
// ──────────────────────────────────────────────────────────────────────────────

struct AgentData {
    ip: Option<String>,
    mounts: Vec<VmDiskMount>,
    processes: Vec<VmProcess>,
}

struct SshData {
    services: Vec<VmService>,
    mounts: Vec<VmDiskMount>,
}

fn ssh_connect(ip: &str, cfg: &SshConfig) -> Result<Session> {
    let addr = format!("{}:22", ip);
    let tcp = TcpStream::connect_timeout(
        &addr.parse().context("Parsing SSH addr")?,
        Duration::from_secs(cfg.timeout_secs),
    )
    .with_context(|| format!("TCP connect to {addr}"))?;

    tcp.set_read_timeout(Some(Duration::from_secs(cfg.timeout_secs)))?;

    let mut sess = Session::new().context("Creating SSH session")?;
    sess.set_tcp_stream(tcp);
    sess.handshake().context("SSH handshake")?;

    // Try public-key auth
    sess.userauth_pubkey_file(
        &cfg.user,
        None,
        Path::new(&cfg.private_key_path),
        None,
    )
    .with_context(|| format!("SSH auth to {ip}"))?;

    if !sess.authenticated() {
        anyhow::bail!("SSH auth failed to {ip}");
    }

    Ok(sess)
}

fn ssh_exec(sess: &Session, cmd: &str) -> Result<String> {
    let mut channel = sess.channel_session().context("SSH channel")?;
    channel.exec(cmd).with_context(|| format!("exec: {cmd}"))?;
    let mut output = String::new();
    channel.read_to_string(&mut output).context("SSH read")?;
    channel.wait_close().ok();
    Ok(output)
}

fn ssh_collect(ip: &str, cfg: &SshConfig) -> Result<SshData> {
    let sess = ssh_connect(ip, cfg)?;

    // Services
    let svc_out = ssh_exec(
        &sess,
        "systemctl list-units --type=service --no-pager --no-legend \
         --output=json 2>/dev/null || rc-status --nocolor 2>/dev/null",
    )?;
    let services = parse_systemctl_json(&svc_out);

    // Disk mounts
    let df_out = ssh_exec(
        &sess,
        "df --output=source,fstype,size,used,avail,pcent,target \
         -k --block-size=1 2>/dev/null",
    )?;
    let mounts = parse_df_output(&df_out);

    Ok(SshData { services, mounts })
}

// ──────────────────────────────────────────────────────────────────────────────
// Parsers
// ──────────────────────────────────────────────────────────────────────────────

fn parse_df_output(output: &str) -> Vec<VmDiskMount> {
    let skip_fstypes = ["tmpfs", "devtmpfs", "proc", "sysfs", "devpts", "cgroup2", "squashfs"];
    let mut mounts = Vec::new();

    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 7 {
            continue;
        }
        if skip_fstypes.contains(&parts[1]) {
            continue;
        }
        mounts.push(VmDiskMount {
            fstype: parts[1].to_string(),
            total: parts[2].parse().unwrap_or(0),
            used: parts[3].parse().unwrap_or(0),
            avail: parts[4].parse().unwrap_or(0),
            use_pct: parts[5].trim_end_matches('%').parse::<f64>().unwrap_or(0.0),
            mountpoint: parts[6].to_string(),
        });
    }
    mounts
}

fn parse_ps_output(output: &str) -> Vec<VmProcess> {
    output
        .lines()
        .take(20)
        .filter_map(|line| {
            let p: Vec<&str> = line.split_whitespace().collect();
            if p.len() < 4 {
                return None;
            }
            Some(VmProcess {
                pid: p[0].parse().ok()?,
                name: p[1].to_string(),
                cpu_pct: p[2].parse().unwrap_or(0.0),
                mem_rss_kb: p[3].parse().unwrap_or(0),
            })
        })
        .collect()
}

fn parse_systemctl_json(output: &str) -> Vec<VmService> {
    #[derive(serde::Deserialize)]
    struct Unit {
        unit: String,
        active: String,
        sub: String,
    }

    serde_json::from_str::<Vec<Unit>>(output)
        .unwrap_or_default()
        .into_iter()
        .map(|u| VmService {
            name: u.unit,
            active: u.active == "active",
            status: u.sub,
        })
        .collect()
}
