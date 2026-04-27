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
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::net::TcpStream;
use std::path::Path;
use std::time::Duration;
use tokio::process::Command;
use tracing::{debug, warn};

use crate::config::SshConfig;
use crate::proxmox_api::{GuestAgentFsInfo, ProxmoxClient};

#[derive(Debug, Clone, Serialize)]
pub struct VmDetailedStats {
    pub vmid: u32,
    pub name: String,
    pub ip_address: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
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
    pub load: String,
    pub state: String,
    pub sub_state: String,
    pub description: String,
    pub running: bool,
    pub failed: bool,
    pub classification: String,
    pub ports: Vec<String>,
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
            os_name: None,
            os_version: None,
            services: vec![],
            disk_mounts: vec![],
            top_processes: vec![],
            agent_available: false,
            ssh_available: false,
        };

        // Try QEMU guest agent first
        match self.collect_via_agent(node, vmid, name).await {
            Ok(agent_data) => {
                stats.agent_available = true;
                stats.ip_address = agent_data.ip;
                stats.os_name = agent_data.os_name;
                stats.os_version = agent_data.os_version;
                stats.services = agent_data.services;
                stats.disk_mounts = agent_data.mounts;
                stats.top_processes = agent_data.processes;

                // If we have an IP, also try SSH for service status and logs
                if let Some(ip) = stats.ip_address.clone() {
                    match self.collect_via_ssh(&ip).await {
                        Ok(ssh_data) => {
                            stats.ssh_available = true;
                            if stats.os_name.is_none() {
                                stats.os_name = ssh_data.os_name;
                                stats.os_version = ssh_data.os_version;
                            }
                            if !ssh_data.services.is_empty() {
                                stats.services = ssh_data.services;
                            }
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
                            stats.os_name = ssh_data.os_name;
                            stats.os_version = ssh_data.os_version;
                            stats.services = ssh_data.services;
                            stats.disk_mounts = ssh_data.mounts;
                        }
                        Err(e) => warn!("SSH fallback failed for vm {vmid}: {e}"),
                    }
                }
            }
        }

        if stats.ip_address.is_none() {
            stats.ip_address = discover_ip_from_host(vmid).await;
        }

        if !stats.ssh_available {
            if let Some(ip) = stats.ip_address.clone() {
                match self.collect_via_ssh(&ip).await {
                    Ok(ssh_data) => {
                        stats.ssh_available = true;
                        if stats.os_name.is_none() {
                            stats.os_name = ssh_data.os_name;
                            stats.os_version = ssh_data.os_version;
                        }
                        if stats.services.is_empty() {
                            stats.services = ssh_data.services;
                        }
                        if stats.disk_mounts.is_empty() {
                            stats.disk_mounts = ssh_data.mounts;
                        }
                    }
                    Err(e) => debug!("SSH after IP discovery failed for vm {} ({ip}): {e}", vmid),
                }
            }
        }

        stats
    }

    // ── Guest Agent path ───────────────────────────────────────────────────

    async fn collect_via_agent(&self, node: &str, vmid: u32, name: &str) -> Result<AgentData> {
        self.client.vm_agent_ping(node, vmid).await?;

        let ip = self.client.vm_agent_ip(node, vmid).await;

        let (mut os_name, mut os_version) = self
            .client
            .vm_agent_os_info(node, vmid)
            .await
            .map(|info| (info.name, info.version))
            .unwrap_or((None, None));

        let mut mounts = self
            .client
            .vm_agent_fs_info(node, vmid)
            .await
            .map(agent_mounts_to_vm_mounts)
            .unwrap_or_default();

        if mounts.is_empty() {
            match self
                .client
                .vm_agent_exec_shell(
                    node,
                    vmid,
                    "df --output=source,fstype,size,used,avail,pcent,target -k --block-size=1 --no-sync",
                )
                .await
            {
                Ok(fs_json) => mounts = parse_df_output(&fs_json),
                Err(e) => warn!("VM {vmid} guest-agent exec_error df: {e}"),
            }
        }

        // Get process list via agent exec
        let ps_json = match self
            .client
            .vm_agent_exec_shell(
                node,
                vmid,
                "ps -eo pid,comm,pcpu,rss --no-headers --sort=-pcpu",
            )
            .await
        {
            Ok(output) => output,
            Err(e) => {
                warn!("VM {vmid} guest-agent exec_error ps: {e}");
                String::new()
            }
        };

        let processes = parse_ps_output(&ps_json);

        // Service discovery through QEMU Guest Agent keeps VM monitoring agentless:
        // no Sentinel sidecar inside the VM and no SSH key requirement. Collect
        // every unit, then collect running/failed views too so distro differences
        // do not hide state.
        let svc_json = self.collect_agent_service_output(node, vmid, name).await;

        let port_output = match self
            .client
            .vm_agent_exec_shell(node, vmid, "ss -lntup 2>/dev/null || true")
            .await
        {
            Ok(output) => output,
            Err(e) => {
                warn!("VM {vmid} guest-agent exec_error ss: {e}");
                String::new()
            }
        };
        let services = attach_ports(
            parse_services_output(&svc_json),
            &parse_ss_output(&port_output),
        );
        if services.is_empty() {
            warn!(
                "VM {name} ({vmid}) service discovery returned 0 parsed services; os={:?} {:?}; first_output_lines={:?}",
                os_name,
                os_version,
                first_lines(&svc_json, 50)
            );
        }

        if os_name.is_none() || os_version.is_none() {
            let os_release = match self
                .client
                .vm_agent_exec_shell(node, vmid, "cat /etc/os-release 2>/dev/null || true")
                .await
            {
                Ok(output) => output,
                Err(e) => {
                    warn!("VM {vmid} guest-agent exec_error os-release: {e}");
                    String::new()
                }
            };
            let (release_name, release_version) = parse_os_release(&os_release);
            if os_name.is_none() {
                os_name = release_name;
            }
            if os_version.is_none() {
                os_version = release_version;
            }
        }

        Ok(AgentData {
            ip,
            os_name,
            os_version,
            mounts,
            processes,
            services,
        })
    }

    async fn collect_agent_service_output(&self, node: &str, vmid: u32, name: &str) -> String {
        let commands = [
            (
                "all_plain",
                "systemctl list-units --type=service --all --no-pager --no-legend --plain 2>&1",
            ),
            (
                "all_default",
                "systemctl list-units --type=service --all --no-pager --no-legend 2>&1",
            ),
            (
                "running_plain",
                "systemctl list-units --type=service --state=running --no-pager --no-legend --plain 2>&1",
            ),
            (
                "failed_plain",
                "systemctl --failed --type=service --no-pager --no-legend --plain 2>&1",
            ),
        ];
        let mut combined = String::new();
        for (label, command) in commands {
            match self.client.vm_agent_exec_shell(node, vmid, command).await {
                Ok(output) => {
                    if !output.trim().is_empty() {
                        debug!(
                            "VM {name} ({vmid}) service command {label} returned {} bytes",
                            output.len()
                        );
                        combined.push_str(&output);
                        combined.push('\n');
                    }
                    if label == "all_default" && !parse_services_output(&combined).is_empty() {
                        continue;
                    }
                }
                Err(e) => warn!("VM {name} ({vmid}) guest-agent exec_error services {label}: {e}"),
            }
        }

        if parse_services_output(&combined).is_empty() {
            match self
                .client
                .vm_agent_exec_shell(node, vmid, "rc-status --nocolor 2>&1 || true")
                .await
            {
                Ok(output) => combined.push_str(&output),
                Err(e) => warn!("VM {name} ({vmid}) guest-agent exec_error rc-status: {e}"),
            }
        }
        combined
    }

    // ── SSH path ───────────────────────────────────────────────────────────

    async fn collect_via_ssh(&self, ip: &str) -> Result<SshData> {
        let cfg = self.ssh_cfg.clone();
        let ip_owned = ip.to_string();

        // SSH is blocking; run on thread pool
        tokio::task::spawn_blocking(move || ssh_collect(&ip_owned, &cfg))
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
                services.push(vm_service_from_parts(name, "", &status, &status, ""));
            }
            Ok(services)
        })
        .await
        .context("SSH check-services")?
    }
}

async fn discover_ip_from_host(vmid: u32) -> Option<String> {
    let cfg = Command::new("qm")
        .args(["config", &vmid.to_string()])
        .output()
        .await
        .ok()?;
    if !cfg.status.success() {
        return None;
    }

    let cfg_text = String::from_utf8_lossy(&cfg.stdout);
    let macs: Vec<String> = cfg_text
        .lines()
        .filter(|line| line.starts_with("net"))
        .filter_map(|line| line.split_once(':').map(|(_, rest)| rest))
        .filter_map(|rest| rest.split_once('=').map(|(_, after)| after))
        .filter_map(|after| after.split(',').next())
        .map(|mac| mac.trim().to_lowercase())
        .filter(|mac| mac.len() == 17)
        .collect();

    if macs.is_empty() {
        return None;
    }

    let neigh = Command::new("ip")
        .args(["neigh", "show"])
        .output()
        .await
        .ok()?;
    if !neigh.status.success() {
        return None;
    }

    let neigh_text = String::from_utf8_lossy(&neigh.stdout);
    for line in neigh_text.lines() {
        let lower = line.to_lowercase();
        if macs.iter().any(|mac| lower.contains(mac)) {
            return line.split_whitespace().next().map(str::to_string);
        }
    }
    None
}

// ──────────────────────────────────────────────────────────────────────────────
// SSH helpers (sync, run in spawn_blocking)
// ──────────────────────────────────────────────────────────────────────────────

struct AgentData {
    ip: Option<String>,
    os_name: Option<String>,
    os_version: Option<String>,
    mounts: Vec<VmDiskMount>,
    processes: Vec<VmProcess>,
    services: Vec<VmService>,
}

struct SshData {
    os_name: Option<String>,
    os_version: Option<String>,
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
    sess.userauth_pubkey_file(&cfg.user, None, Path::new(&cfg.private_key_path), None)
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
        "(systemctl list-units --type=service --all --no-pager --no-legend --plain 2>/dev/null; \
         systemctl list-units --type=service --state=running --no-pager --no-legend --plain 2>/dev/null; \
         systemctl --failed --type=service --no-pager --no-legend --plain 2>/dev/null) \
         || rc-status --nocolor 2>/dev/null",
    )?;
    let ports = ssh_exec(&sess, "ss -lntup 2>/dev/null || true").unwrap_or_default();
    let services = attach_ports(parse_services_output(&svc_out), &parse_ss_output(&ports));

    let os_out = ssh_exec(&sess, "cat /etc/os-release 2>/dev/null || true")?;
    let (os_name, os_version) = parse_os_release(&os_out);

    // Disk mounts
    let df_out = ssh_exec(
        &sess,
        "df --output=source,fstype,size,used,avail,pcent,target \
         -k --block-size=1 2>/dev/null",
    )?;
    let mounts = parse_df_output(&df_out);

    Ok(SshData {
        os_name,
        os_version,
        services,
        mounts,
    })
}

// ──────────────────────────────────────────────────────────────────────────────
// Parsers
// ──────────────────────────────────────────────────────────────────────────────

fn parse_df_output(output: &str) -> Vec<VmDiskMount> {
    let mut mounts = Vec::new();

    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 7 {
            continue;
        }
        if is_pseudo_filesystem(parts[1], parts[6], false) {
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

fn agent_mounts_to_vm_mounts(mounts: Vec<GuestAgentFsInfo>) -> Vec<VmDiskMount> {
    mounts
        .into_iter()
        .filter(|mount| !is_pseudo_filesystem(&mount.fstype, &mount.mountpoint, false))
        .map(|mount| VmDiskMount {
            mountpoint: mount.mountpoint,
            total: mount.total,
            used: mount.used,
            avail: mount.avail,
            use_pct: mount.use_pct,
            fstype: mount.fstype,
        })
        .collect()
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

fn parse_services_output(output: &str) -> Vec<VmService> {
    let trimmed = output.trim_start();
    if trimmed.starts_with('[') {
        return dedupe_services(parse_systemctl_json(output));
    }
    let systemd = parse_systemctl_plain(output);
    if !systemd.is_empty() {
        return dedupe_services(systemd);
    }
    dedupe_services(parse_openrc_output(output))
}

fn parse_os_release(output: &str) -> (Option<String>, Option<String>) {
    let mut name = None;
    let mut version = None;

    for line in output.lines() {
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let value = raw_value.trim().trim_matches('"').to_string();
        match key {
            "NAME" => name = Some(value),
            "VERSION_ID" => version = Some(value),
            "VERSION" if version.is_none() => version = Some(value),
            _ => {}
        }
    }

    (name, version)
}

fn parse_systemctl_json(output: &str) -> Vec<VmService> {
    #[derive(serde::Deserialize)]
    struct Unit {
        unit: String,
        #[serde(default)]
        load: String,
        active: String,
        sub: String,
        #[serde(default)]
        description: String,
    }

    serde_json::from_str::<Vec<Unit>>(output)
        .unwrap_or_default()
        .into_iter()
        .map(|u| vm_service_from_parts(&u.unit, &u.load, &u.active, &u.sub, &u.description))
        .collect()
}

fn parse_openrc_output(output: &str) -> Vec<VmService> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('[') {
                return None;
            }
            let bracket = line.rfind('[')?;
            let name = line[..bracket].trim().to_string();
            let state = line[bracket + 1..].trim_end_matches(']').trim().to_string();
            if name.is_empty() {
                return None;
            }
            Some(vm_service_from_parts(&name, "", &state, &state, ""))
        })
        .collect()
}

fn parse_systemctl_plain(output: &str) -> Vec<VmService> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("UNIT ") || line.starts_with("LOAD ") {
                return None;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 || !parts[0].ends_with(".service") {
                return None;
            }
            let description = parts.get(4..).map(|p| p.join(" ")).unwrap_or_default();
            Some(vm_service_from_parts(
                parts[0],
                parts[1],
                parts[2],
                parts[3],
                &description,
            ))
        })
        .collect()
}

fn vm_service_from_parts(
    name: &str,
    load: &str,
    state: &str,
    sub_state: &str,
    description: &str,
) -> VmService {
    let state = state.trim().to_ascii_lowercase();
    let sub_state = sub_state.trim().to_ascii_lowercase();
    let running = matches!(state.as_str(), "active" | "started")
        && matches!(sub_state.as_str(), "running" | "started" | "active");
    let failed = state == "failed" || sub_state == "failed";
    let classification = classify_service(name);

    VmService {
        name: name.to_string(),
        active: running,
        status: sub_state.clone(),
        load: load.to_string(),
        state,
        sub_state,
        description: description.to_string(),
        running,
        failed,
        classification,
        ports: vec![],
    }
}

fn dedupe_services(services: Vec<VmService>) -> Vec<VmService> {
    let mut map: BTreeMap<String, VmService> = BTreeMap::new();
    for service in services {
        let key = normalize_service_key(&service.name);
        match map.get(&key) {
            Some(existing) if service_rank(existing) <= service_rank(&service) => {}
            _ => {
                map.insert(key, service);
            }
        }
    }
    map.into_values().collect()
}

fn service_rank(service: &VmService) -> u8 {
    if service.failed {
        0
    } else if service.running {
        1
    } else if service.state == "active" {
        2
    } else if service.state == "inactive" {
        3
    } else {
        4
    }
}

fn attach_ports(
    mut services: Vec<VmService>,
    ports: &HashMap<String, Vec<String>>,
) -> Vec<VmService> {
    for service in &mut services {
        let mut found = ports_for_service(&service.name, ports);
        sort_ports(&mut found);
        found.dedup();
        service.ports = found;
    }
    services
}

fn sort_ports(ports: &mut [String]) {
    ports.sort_by(|a, b| {
        let a_num = a.parse::<u16>();
        let b_num = b.parse::<u16>();
        match (a_num, b_num) {
            (Ok(a), Ok(b)) => a.cmp(&b),
            _ => a.cmp(b),
        }
    });
}

fn parse_ss_output(output: &str) -> HashMap<String, Vec<String>> {
    let mut ports: HashMap<String, Vec<String>> = HashMap::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with("Netid") {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        let Some(port) = extract_port(parts[4]) else {
            continue;
        };
        let process_names = extract_process_names(line);
        for process in process_names {
            ports
                .entry(normalize_service_key(&process))
                .or_default()
                .push(port.clone());
        }
    }
    ports
}

fn extract_port(local_addr: &str) -> Option<String> {
    let addr = local_addr.trim_matches('"');
    let port = addr.rsplit_once(':')?.1.trim_end_matches(']').to_string();
    if port.is_empty() || port == "*" {
        None
    } else {
        Some(port)
    }
}

fn extract_process_names(line: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find("((\"") {
        rest = &rest[start + 3..];
        let Some(end) = rest.find('"') else {
            break;
        };
        let name = &rest[..end];
        if !name.is_empty() {
            names.push(name.to_string());
        }
        rest = &rest[end + 1..];
    }
    names
}

fn ports_for_service(service_name: &str, ports: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut found = Vec::new();
    for key in service_process_keys(service_name) {
        if let Some(values) = ports.get(&key) {
            found.extend(values.iter().cloned());
        }
    }
    found
}

fn service_process_keys(service_name: &str) -> Vec<String> {
    let key = normalize_service_key(service_name);
    let mut keys = vec![key.clone()];
    if key == "ssh" {
        keys.push("sshd".to_string());
    }
    if let Some(version) = key.strip_prefix("php").and_then(|s| s.strip_suffix("-fpm")) {
        keys.push(normalize_service_key(&format!("php-fpm{version}")));
    }
    keys
}

fn classify_service(name: &str) -> String {
    let key = normalize_service_key(name);
    let class = if matches!(key.as_str(), "apache2" | "nginx" | "caddy" | "traefik") {
        "web"
    } else if key.starts_with("php") && key.contains("fpm") {
        "php"
    } else if matches!(
        key.as_str(),
        "mysql" | "mariadb" | "postgresql" | "redis" | "redis-server" | "mongodb" | "mongod"
    ) {
        "database"
    } else if matches!(key.as_str(), "docker" | "containerd" | "podman") {
        "container"
    } else if matches!(key.as_str(), "haproxy" | "keepalived") {
        "proxy/lb"
    } else if matches!(
        key.as_str(),
        "prometheus" | "grafana" | "node_exporter" | "node-exporter" | "zabbix-agent"
    ) {
        "monitoring"
    } else if key.starts_with("systemd-")
        || matches!(
            key.as_str(),
            "dbus"
                | "dbus-broker"
                | "cron"
                | "crond"
                | "rsyslog"
                | "qemu-guest-agent"
                | "fwupd"
                | "fwupd-refresh"
                | "getty"
                | "serial-getty"
                | "chronyd"
                | "networkmanager"
        )
    {
        "system"
    } else {
        "other"
    };
    class.to_string()
}

fn is_pseudo_filesystem(fstype: &str, mountpoint: &str, container_root: bool) -> bool {
    matches!(
        fstype,
        "tmpfs"
            | "devtmpfs"
            | "squashfs"
            | "proc"
            | "sysfs"
            | "cgroup"
            | "cgroup2"
            | "devpts"
            | "securityfs"
            | "pstore"
            | "bpf"
            | "tracefs"
            | "debugfs"
            | "hugetlbfs"
            | "mqueue"
            | "fusectl"
            | "configfs"
    ) || (fstype == "overlay" && !(container_root && mountpoint == "/"))
}

fn first_lines(output: &str, limit: usize) -> Vec<String> {
    output.lines().take(limit).map(str::to_string).collect()
}

fn normalize_service_key(name: &str) -> String {
    name.trim()
        .trim_end_matches(".service")
        .to_ascii_lowercase()
        .replace('@', "-")
}

#[cfg(test)]
mod tests {
    use super::{
        attach_ports, classify_service, parse_openrc_output, parse_services_output,
        parse_ss_output, parse_systemctl_plain,
    };

    #[test]
    fn parses_systemctl_json_services() {
        let services =
            parse_services_output(include_str!("../../tests/fixtures/services/systemctl.json"));
        assert_eq!(services.len(), 3);
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "nginx.service" && svc.active)
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "postgresql.service" && svc.status == "running")
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "redis.service" && !svc.active && svc.status == "failed")
        );
    }

    #[test]
    fn parses_systemctl_plain_services() {
        let services = parse_systemctl_plain(include_str!(
            "../../tests/fixtures/services/systemctl_plain.txt"
        ));
        assert_eq!(services.len(), 3);
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "apache2.service" && svc.active)
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "php8.3-fpm.service" && svc.status == "running")
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "mariadb.service" && !svc.active && svc.status == "dead")
        );
    }

    #[test]
    fn parses_real_qemu_agent_plain_services() {
        let services = parse_systemctl_plain(
            "apache2.service loaded active running The Apache HTTP Server\napparmor.service loaded active exited Load AppArmor profiles\ncron.service loaded active running Regular background program processing daemon\n",
        );
        assert_eq!(services.len(), 3);

        let apache = services
            .iter()
            .find(|svc| svc.name == "apache2.service")
            .expect("apache service");
        assert_eq!(apache.load, "loaded");
        assert_eq!(apache.state, "active");
        assert_eq!(apache.sub_state, "running");
        assert_eq!(apache.description, "The Apache HTTP Server");
        assert!(apache.running);
        assert!(apache.active);

        let apparmor = services
            .iter()
            .find(|svc| svc.name == "apparmor.service")
            .expect("apparmor service");
        assert_eq!(apparmor.state, "active");
        assert_eq!(apparmor.sub_state, "exited");
        assert!(!apparmor.running);
        assert!(!apparmor.failed);
    }

    #[test]
    fn fedora_systemctl_services_parse_and_fwupd_is_system() {
        let services = parse_systemctl_plain(
            "NetworkManager.service loaded active running Network Manager\n\
             dbus-broker.service loaded active running D-Bus System Message Bus\n\
             sshd.service loaded active running OpenSSH server daemon\n\
             qemu-guest-agent.service loaded active running QEMU Guest Agent\n\
             fwupd.service loaded inactive dead Firmware update daemon\n",
        );
        assert_eq!(services.len(), 5);
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "sshd.service" && svc.running)
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "NetworkManager.service" && svc.running)
        );
        assert_eq!(classify_service("fwupd.service"), "system");
    }

    #[test]
    fn classifies_and_maps_ports_for_application_services() {
        let services = parse_services_output(
            "apache2.service loaded active running The Apache HTTP Server\n\
             php8.3-fpm.service loaded active running The PHP 8.3 FastCGI Process Manager\n\
             ssh.service loaded active running OpenBSD Secure Shell server\n\
             systemd-journald.service loaded active running Journal Service\n",
        );
        let ports = parse_ss_output(
            "tcp LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:((\"apache2\",pid=123,fd=4))\n\
             tcp LISTEN 0 511 0.0.0.0:443 0.0.0.0:* users:((\"apache2\",pid=124,fd=4))\n\
             tcp LISTEN 0 128 127.0.0.1:9000 0.0.0.0:* users:((\"php-fpm8.3\",pid=456,fd=8))\n\
             tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=1,fd=3))\n",
        );
        let services = attach_ports(services, &ports);

        let apache = services
            .iter()
            .find(|svc| svc.name == "apache2.service")
            .unwrap();
        assert_eq!(apache.classification, "web");
        assert_eq!(apache.ports, vec!["80", "443"]);

        let php = services
            .iter()
            .find(|svc| svc.name == "php8.3-fpm.service")
            .unwrap();
        assert_eq!(php.classification, "php");
        assert_eq!(php.ports, vec!["9000"]);

        let ssh = services
            .iter()
            .find(|svc| svc.name == "ssh.service")
            .unwrap();
        assert_eq!(ssh.ports, vec!["22"]);
    }

    #[test]
    fn parses_openrc_services() {
        let services =
            parse_openrc_output(include_str!("../../tests/fixtures/services/openrc.txt"));
        assert_eq!(services.len(), 3);
        assert!(services.iter().any(|svc| svc.name == "sshd" && svc.active));
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "redis" && !svc.active && svc.status == "stopped")
        );
    }
}
