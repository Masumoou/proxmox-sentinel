// src/collectors/lxc.rs
//
// Agentless LXC monitoring.
// Techniques used (all from the Proxmox host, zero VM-side agent needed):
//
//   1. cgroup v2 filesystem  →  CPU, memory, I/O, pids
//   2. /proc/<init_pid>/net/ →  network stats per container
//   3. nsenter               →  run commands inside container namespace
//   4. pct exec              →  fallback for service/log queries
//   5. Direct rootfs read    →  /var/lib/lxc/<id>/rootfs/var/log/*

use anyhow::{Context, Result};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use tokio::fs;
use tokio::process::Command;
use tracing::debug;

// ──────────────────────────────────────────────────────────────────────────────
// Output types
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct LxcDetailedStats {
    pub vmid: u32,
    pub name: String,
    pub ip_address: Option<String>,
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub cgroup: CgroupStats,
    pub network: Vec<NetIfaceStats>,
    pub services: Vec<ServiceStatus>,
    pub disk_mounts: Vec<DiskMount>,
    pub processes: Vec<ProcessInfo>,
    pub init_pid: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CgroupStats {
    // CPU
    pub cpu_usage_usec: u64, // cumulative microseconds used
    pub cpu_user_usec: u64,
    pub cpu_system_usec: u64,
    pub cpu_nr_periods: u64,
    pub cpu_nr_throttled: u64,
    // Memory
    pub mem_current: u64, // bytes currently used
    pub mem_peak: u64,
    pub mem_limit: u64, // 0 = unlimited
    pub mem_swap_current: u64,
    pub mem_anon: u64, // anonymous (heap/stack) pages
    pub mem_file: u64, // file-backed pages (page cache)
    // I/O (blkio)
    pub io_read_bytes: u64,
    pub io_write_bytes: u64,
    pub io_read_ops: u64,
    pub io_write_ops: u64,
    // PIDs
    pub pid_current: u64,
    pub pid_limit: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetIfaceStats {
    pub iface: String,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatus {
    pub name: String,
    pub load: String,
    pub state: String,     // "active", "inactive", "failed", "activating"
    pub sub_state: String, // "running", "dead", "exited"
    pub enabled: bool,
    pub description: String,
    pub running: bool,
    pub failed: bool,
    pub classification: String,
    pub ports: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DiskMount {
    pub mountpoint: String,
    pub device: String,
    pub fstype: String,
    pub used: u64,
    pub total: u64,
    pub avail: u64,
    pub use_pct: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_pct: f64,
    pub mem_rss_kb: u64,
    pub state: char,
}

// ──────────────────────────────────────────────────────────────────────────────
// Collector
// ──────────────────────────────────────────────────────────────────────────────

pub struct LxcCollector;

impl LxcCollector {
    /// Main entry point: collect everything for a running LXC
    pub async fn collect(vmid: u32, name: &str) -> LxcDetailedStats {
        let init_pid = Self::find_init_pid(vmid).await;

        let cgroup = Self::read_cgroup(vmid).await.unwrap_or_default();
        let network = match init_pid {
            Some(pid) => Self::read_net_stats(pid).await.unwrap_or_default(),
            None => vec![],
        };
        let mut services = Self::list_services(vmid).await.unwrap_or_default();
        let ports = Self::list_listening_ports(vmid).await.unwrap_or_default();
        attach_ports(&mut services, &ports);
        let disk_mounts = Self::read_disk_usage(vmid).await.unwrap_or_default();
        let (os_name, os_version) = Self::read_os_release(vmid).await.unwrap_or((None, None));
        let ip_address = Self::read_ip_address(vmid).await;
        let processes = match init_pid {
            Some(pid) => Self::read_top_processes(pid).await.unwrap_or_default(),
            None => vec![],
        };

        LxcDetailedStats {
            vmid,
            name: name.to_string(),
            ip_address,
            os_name,
            os_version,
            cgroup,
            network,
            services,
            disk_mounts,
            processes,
            init_pid,
        }
    }

    // ── cgroup v2 ──────────────────────────────────────────────────────────

    async fn read_cgroup(vmid: u32) -> Result<CgroupStats> {
        let base = Self::lxc_cgroup_base(vmid)
            .ok_or_else(|| anyhow::anyhow!("No cgroup found for LXC {vmid}"))?;

        let mut stats = CgroupStats::default();

        // CPU stats
        if let Ok(content) = read_file(base.join("cpu.stat")).await {
            for line in content.lines() {
                let mut parts = line.splitn(2, ' ');
                match (parts.next(), parts.next()) {
                    (Some("usage_usec"), Some(v)) => {
                        stats.cpu_usage_usec = v.trim().parse().unwrap_or(0)
                    }
                    (Some("user_usec"), Some(v)) => {
                        stats.cpu_user_usec = v.trim().parse().unwrap_or(0)
                    }
                    (Some("system_usec"), Some(v)) => {
                        stats.cpu_system_usec = v.trim().parse().unwrap_or(0)
                    }
                    (Some("nr_periods"), Some(v)) => {
                        stats.cpu_nr_periods = v.trim().parse().unwrap_or(0)
                    }
                    (Some("nr_throttled"), Some(v)) => {
                        stats.cpu_nr_throttled = v.trim().parse().unwrap_or(0)
                    }
                    _ => {}
                }
            }
        }

        // Memory
        stats.mem_current = read_u64(base.join("memory.current")).await.unwrap_or(0);
        stats.mem_peak = read_u64(base.join("memory.peak")).await.unwrap_or(0);
        // "max" may be literal "max" if unlimited
        stats.mem_limit = read_file(base.join("memory.max"))
            .await
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);
        stats.mem_swap_current = read_u64(base.join("memory.swap.current"))
            .await
            .unwrap_or(0);

        // memory.stat for anon/file breakdown
        if let Ok(content) = read_file(base.join("memory.stat")).await {
            for line in content.lines() {
                let mut parts = line.splitn(2, ' ');
                match (parts.next(), parts.next()) {
                    (Some("anon"), Some(v)) => stats.mem_anon = v.trim().parse().unwrap_or(0),
                    (Some("file"), Some(v)) => stats.mem_file = v.trim().parse().unwrap_or(0),
                    _ => {}
                }
            }
        }

        // I/O — io.stat  format: "major:minor rbytes=N wbytes=N rios=N wios=N ..."
        if let Ok(content) = read_file(base.join("io.stat")).await {
            for line in content.lines() {
                let fields: HashMap<&str, u64> = line
                    .split_whitespace()
                    .skip(1) // skip "major:minor"
                    .filter_map(|kv| {
                        let mut it = kv.splitn(2, '=');
                        let k = it.next()?;
                        let v = it.next()?.parse::<u64>().ok()?;
                        Some((k, v))
                    })
                    .collect();
                stats.io_read_bytes += fields.get("rbytes").copied().unwrap_or(0);
                stats.io_write_bytes += fields.get("wbytes").copied().unwrap_or(0);
                stats.io_read_ops += fields.get("rios").copied().unwrap_or(0);
                stats.io_write_ops += fields.get("wios").copied().unwrap_or(0);
            }
        }

        // PIDs
        stats.pid_current = read_u64(base.join("pids.current")).await.unwrap_or(0);
        stats.pid_limit = read_file(base.join("pids.max"))
            .await
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(0);

        Ok(stats)
    }

    // ── Find container's init PID from host /proc ──────────────────────────

    async fn read_os_release(vmid: u32) -> Result<(Option<String>, Option<String>)> {
        let out = Command::new("pct")
            .args(["exec", &vmid.to_string(), "--", "cat", "/etc/os-release"])
            .output()
            .await
            .context("pct exec cat /etc/os-release")?;
        if !out.status.success() {
            return Ok((None, None));
        }
        Ok(parse_os_release(&String::from_utf8_lossy(&out.stdout)))
    }

    async fn read_ip_address(vmid: u32) -> Option<String> {
        let out = Command::new("pct")
            .args([
                "exec",
                &vmid.to_string(),
                "--",
                "sh",
                "-lc",
                "hostname -I 2>/dev/null || ip -4 -o addr show scope global 2>/dev/null",
            ])
            .output()
            .await
            .ok()?;
        if !out.status.success() {
            return None;
        }
        parse_first_ipv4(&String::from_utf8_lossy(&out.stdout))
    }

    async fn find_init_pid(vmid: u32) -> Option<u32> {
        let procs_path = Self::lxc_cgroup_base(vmid)?.join("cgroup.procs");
        let content = fs::read_to_string(&procs_path).await.ok()?;
        // First PID is usually the container init
        content.lines().next()?.trim().parse::<u32>().ok()
    }

    fn lxc_cgroup_base(vmid: u32) -> Option<PathBuf> {
        let candidates = [
            format!("/sys/fs/cgroup/lxc/{vmid}"),
            format!("/sys/fs/cgroup/lxc/{vmid}.scope"),
            format!("/sys/fs/cgroup/machine.slice/lxc-{vmid}.scope"),
            format!("/sys/fs/cgroup/system.slice/pve-container@{vmid}.service"),
        ];

        candidates
            .into_iter()
            .map(PathBuf::from)
            .find(|path| path.join("cgroup.procs").exists() || path.join("memory.current").exists())
            .or_else(|| {
                debug!("cgroup path not found for LXC {vmid}");
                None
            })
    }

    // ── Network stats via /proc/<init_pid>/net/dev ─────────────────────────

    async fn read_net_stats(init_pid: u32) -> Result<Vec<NetIfaceStats>> {
        let path = format!("/proc/{}/net/dev", init_pid);
        let content = fs::read_to_string(&path)
            .await
            .with_context(|| format!("Reading {path}"))?;

        let mut ifaces = Vec::new();
        // Skip 2 header lines
        for line in content.lines().skip(2) {
            let line = line.trim();
            let colon = match line.find(':') {
                Some(p) => p,
                None => continue,
            };
            let iface = line[..colon].trim().to_string();
            if iface == "lo" {
                continue;
            }
            let nums: Vec<u64> = line[colon + 1..]
                .split_whitespace()
                .filter_map(|s| s.parse().ok())
                .collect();

            if nums.len() < 16 {
                continue;
            }
            ifaces.push(NetIfaceStats {
                iface,
                rx_bytes: nums[0],
                rx_packets: nums[1],
                rx_errors: nums[2],
                tx_bytes: nums[8],
                tx_packets: nums[9],
                tx_errors: nums[10],
            });
        }
        Ok(ifaces)
    }

    // ── Service status via nsenter + systemctl ─────────────────────────────

    async fn list_services(vmid: u32) -> Result<Vec<ServiceStatus>> {
        // pct exec is the safe, supported way to run commands inside LXC
        let out = Command::new("pct")
            .args([
                "exec",
                &vmid.to_string(),
                "--",
                "sh",
                "-lc",
                "(systemctl list-units --type=service --all --no-pager --no-legend --plain 2>/dev/null; systemctl list-units --type=service --state=running --no-pager --no-legend --plain 2>/dev/null; systemctl --failed --type=service --no-pager --no-legend --plain 2>/dev/null) || rc-status --nocolor 2>/dev/null",
            ])
            .output()
            .await
            .context("pct exec systemctl")?;

        if !out.status.success() {
            // Fallback: non-systemd container (OpenRC, etc.)
            return Self::list_services_openrc(vmid).await;
        }

        #[derive(serde::Deserialize)]
        struct UnitJson {
            unit: String,
            #[serde(default)]
            load: String,
            active: String,
            sub: String,
            #[serde(default)]
            description: String,
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.trim_start().starts_with('[') {
            let units: Vec<UnitJson> = serde_json::from_str(&stdout).unwrap_or_default();

            return Ok(dedupe_services(
                units
                    .into_iter()
                    .map(|u| {
                        service_from_parts(&u.unit, &u.load, &u.active, &u.sub, &u.description)
                    })
                    .collect(),
            ));
        }

        Ok(dedupe_services(parse_services_text(&stdout)))
    }

    async fn list_services_openrc(vmid: u32) -> Result<Vec<ServiceStatus>> {
        let out = Command::new("pct")
            .args(["exec", &vmid.to_string(), "--", "rc-status", "--nocolor"])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut services = Vec::new();
        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with('[') || line.is_empty() {
                continue;
            }
            // Format: " service_name    [ started ]"
            if let Some(bracket) = line.rfind('[') {
                let name = line[..bracket].trim().to_string();
                let state = line[bracket + 1..].trim_end_matches(']').trim().to_string();
                services.push(service_from_parts(&name, "loaded", &state, &state, ""));
            }
        }
        Ok(dedupe_services(services))
    }

    async fn list_listening_ports(vmid: u32) -> Result<HashMap<String, Vec<String>>> {
        let out = Command::new("pct")
            .args([
                "exec",
                &vmid.to_string(),
                "--",
                "sh",
                "-lc",
                "ss -lntup 2>/dev/null || true",
            ])
            .output()
            .await?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(parse_ss_output(&stdout))
    }

    // ── Disk usage via df inside the container ─────────────────────────────

    async fn read_disk_usage(vmid: u32) -> Result<Vec<DiskMount>> {
        let out = Command::new("pct")
            .args([
                "exec",
                &vmid.to_string(),
                "--",
                "df",
                "--output=source,fstype,size,used,avail,pcent,target",
                "-k",
                "--block-size=1", // bytes
            ])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut mounts = Vec::new();

        for line in stdout.lines().skip(1) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 7 {
                continue;
            }
            let fstype = parts[1];
            // Skip pseudo filesystems
            if matches!(
                fstype,
                "tmpfs" | "devtmpfs" | "proc" | "sysfs" | "devpts" | "cgroup2"
            ) {
                continue;
            }
            let total: u64 = parts[2].parse().unwrap_or(0);
            let used: u64 = parts[3].parse().unwrap_or(0);
            let avail: u64 = parts[4].parse().unwrap_or(0);
            let use_pct: f64 = parts[5].trim_end_matches('%').parse::<f64>().unwrap_or(0.0);

            mounts.push(DiskMount {
                device: parts[0].to_string(),
                fstype: fstype.to_string(),
                total,
                used,
                avail,
                use_pct,
                mountpoint: parts[6].to_string(),
            });
        }
        Ok(mounts)
    }

    // ── Top processes via /proc namespace ─────────────────────────────────

    async fn read_top_processes(init_pid: u32) -> Result<Vec<ProcessInfo>> {
        // Read /proc/<init_pid>/root/proc/ which is the container's /proc
        // Or use nsenter to read the container's process list
        let out = Command::new("nsenter")
            .args([
                "-t",
                &init_pid.to_string(),
                "-m",
                "-p", // mount and pid namespaces
                "--",
                "ps",
                "-eo",
                "pid,comm,pcpu,rss,state",
                "--no-headers",
                "--sort=-pcpu",
            ])
            .output()
            .await?;

        let stdout = String::from_utf8_lossy(&out.stdout);
        let mut procs = Vec::new();

        for line in stdout.lines().take(20) {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 5 {
                continue;
            }
            procs.push(ProcessInfo {
                pid: parts[0].parse().unwrap_or(0),
                name: parts[1].to_string(),
                cpu_pct: parts[2].parse().unwrap_or(0.0),
                mem_rss_kb: parts[3].parse().unwrap_or(0),
                state: parts[4].chars().next().unwrap_or('?'),
            });
        }
        Ok(procs)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Log reading from LXC rootfs (host-side, no agent)
// ──────────────────────────────────────────────────────────────────────────────

/// Read the last N lines of a log file from inside an LXC rootfs
/// without entering the container at all.
#[allow(dead_code)]
pub async fn tail_lxc_log(vmid: u32, log_path: &str, lines: usize) -> Result<Vec<String>> {
    // LXC rootfs is at /var/lib/lxc/<vmid>/rootfs/ on the host
    let host_path = format!("/var/lib/lxc/{}/rootfs{}", vmid, log_path);
    tail_file(&host_path, lines).await
}

/// Watch a log file inside an LXC rootfs using inotify (via tokio notify crate)
#[allow(dead_code)]
pub fn lxc_log_host_path(vmid: u32, log_path: &str) -> PathBuf {
    PathBuf::from(format!("/var/lib/lxc/{}/rootfs{}", vmid, log_path))
}

// ──────────────────────────────────────────────────────────────────────────────
// Helpers
// ──────────────────────────────────────────────────────────────────────────────

async fn read_file(path: impl AsRef<Path>) -> Result<String> {
    fs::read_to_string(path.as_ref())
        .await
        .map(|s| s.trim_end().to_string())
        .with_context(|| format!("read {}", path.as_ref().display()))
}

async fn read_u64(path: impl AsRef<Path>) -> Result<u64> {
    read_file(path).await?.trim().parse().map_err(Into::into)
}

fn parse_services_text(output: &str) -> Vec<ServiceStatus> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with("UNIT ") || line.starts_with("LOAD ") {
                return None;
            }

            if let Some(bracket) = line.rfind('[') {
                let name = line[..bracket].trim().to_string();
                let state = line[bracket + 1..].trim_end_matches(']').trim().to_string();
                if name.is_empty() {
                    return None;
                }
                return Some(service_from_parts(&name, "loaded", &state, &state, ""));
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 || !parts[0].ends_with(".service") {
                return None;
            }

            let description = parts.get(4..).map(|p| p.join(" ")).unwrap_or_default();
            Some(service_from_parts(
                parts[0],
                parts[1],
                parts[2],
                parts[3],
                &description,
            ))
        })
        .collect()
}

fn service_from_parts(
    name: &str,
    load: &str,
    state: &str,
    sub_state: &str,
    description: &str,
) -> ServiceStatus {
    let state = state.trim().to_ascii_lowercase();
    let sub_state = sub_state.trim().to_ascii_lowercase();
    let running = matches!(state.as_str(), "active" | "started")
        && matches!(sub_state.as_str(), "running" | "started" | "active");
    let failed = state == "failed" || sub_state == "failed";
    ServiceStatus {
        name: name.to_string(),
        load: load.to_string(),
        state,
        sub_state,
        enabled: load == "loaded",
        description: description.to_string(),
        running,
        failed,
        classification: classify_service(name),
        ports: vec![],
    }
}

fn dedupe_services(services: Vec<ServiceStatus>) -> Vec<ServiceStatus> {
    let mut map: BTreeMap<String, ServiceStatus> = BTreeMap::new();
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

fn service_rank(service: &ServiceStatus) -> u8 {
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

fn attach_ports(services: &mut [ServiceStatus], ports: &HashMap<String, Vec<String>>) {
    for service in services {
        let mut found = ports_for_service(&service.name, ports);
        sort_ports(&mut found);
        found.dedup();
        service.ports = found;
    }
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
        for process in extract_process_names(line) {
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
            "dbus" | "cron" | "rsyslog" | "qemu-guest-agent"
        )
    {
        "system"
    } else {
        "other"
    };
    class.to_string()
}

fn normalize_service_key(name: &str) -> String {
    name.trim()
        .trim_end_matches(".service")
        .to_ascii_lowercase()
        .replace('@', "-")
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

fn parse_first_ipv4(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .filter_map(|token| token.split('/').next())
        .find(|ip| {
            ip.parse::<std::net::Ipv4Addr>()
                .map(|addr| !addr.is_loopback() && !addr.is_link_local())
                .unwrap_or(false)
        })
        .map(str::to_string)
}

#[allow(dead_code)]
async fn tail_file(path: &str, lines: usize) -> Result<Vec<String>> {
    let out = Command::new("tail")
        .args(["-n", &lines.to_string(), path])
        .output()
        .await
        .with_context(|| format!("tail {path}"))?;
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{attach_ports, parse_first_ipv4, parse_services_text, parse_ss_output};

    #[test]
    fn parses_first_global_ipv4() {
        assert_eq!(
            parse_first_ipv4("127.0.0.1 10.10.207.45 169.254.1.1"),
            Some("10.10.207.45".to_string())
        );
    }

    #[test]
    fn parses_lxc_systemctl_plain_services() {
        let services = parse_services_text(include_str!(
            "../../tests/fixtures/services/systemctl_plain.txt"
        ));
        assert_eq!(services.len(), 3);
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "apache2.service" && svc.state == "active")
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "mariadb.service" && svc.sub_state == "dead")
        );
    }

    #[test]
    fn parses_lxc_openrc_services() {
        let services =
            parse_services_text(include_str!("../../tests/fixtures/services/openrc.txt"));
        assert_eq!(services.len(), 3);
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "sshd" && svc.state == "started")
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "redis" && svc.state == "stopped")
        );
    }

    #[test]
    fn maps_lxc_service_ports() {
        let mut services = parse_services_text(
            "apache2.service loaded active running The Apache HTTP Server\n\
             php8.3-fpm.service loaded active running The PHP 8.3 FastCGI Process Manager\n\
             ssh.service loaded active running OpenBSD Secure Shell server\n",
        );
        let ports = parse_ss_output(
            "tcp LISTEN 0 511 0.0.0.0:80 0.0.0.0:* users:((\"apache2\",pid=123,fd=4))\n\
             tcp LISTEN 0 128 127.0.0.1:9000 0.0.0.0:* users:((\"php-fpm8.3\",pid=456,fd=8))\n\
             tcp LISTEN 0 128 0.0.0.0:22 0.0.0.0:* users:((\"sshd\",pid=1,fd=3))\n",
        );
        attach_ports(&mut services, &ports);

        assert!(
            services
                .iter()
                .any(|svc| svc.name == "apache2.service" && svc.ports == vec!["80"])
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "php8.3-fpm.service" && svc.ports == vec!["9000"])
        );
        assert!(
            services
                .iter()
                .any(|svc| svc.name == "ssh.service" && svc.ports == vec!["22"])
        );
    }
}
