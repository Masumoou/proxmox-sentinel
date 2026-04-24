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
use std::collections::HashMap;
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
    pub cpu_usage_usec: u64,   // cumulative microseconds used
    pub cpu_user_usec: u64,
    pub cpu_system_usec: u64,
    pub cpu_nr_periods: u64,
    pub cpu_nr_throttled: u64,
    // Memory
    pub mem_current: u64,      // bytes currently used
    pub mem_peak: u64,
    pub mem_limit: u64,        // 0 = unlimited
    pub mem_swap_current: u64,
    pub mem_anon: u64,         // anonymous (heap/stack) pages
    pub mem_file: u64,         // file-backed pages (page cache)
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
    pub state: String,        // "active", "inactive", "failed", "activating"
    pub sub_state: String,    // "running", "dead", "exited"
    pub enabled: bool,
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
        let services = Self::list_services(vmid).await.unwrap_or_default();
        let disk_mounts = Self::read_disk_usage(vmid).await.unwrap_or_default();
        let (os_name, os_version) = Self::read_os_release(vmid).await.unwrap_or((None, None));
        let processes = match init_pid {
            Some(pid) => Self::read_top_processes(pid).await.unwrap_or_default(),
            None => vec![],
        };

        LxcDetailedStats {
            vmid,
            name: name.to_string(),
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
                    (Some("usage_usec"), Some(v)) => stats.cpu_usage_usec = v.trim().parse().unwrap_or(0),
                    (Some("user_usec"), Some(v)) => stats.cpu_user_usec = v.trim().parse().unwrap_or(0),
                    (Some("system_usec"), Some(v)) => stats.cpu_system_usec = v.trim().parse().unwrap_or(0),
                    (Some("nr_periods"), Some(v)) => stats.cpu_nr_periods = v.trim().parse().unwrap_or(0),
                    (Some("nr_throttled"), Some(v)) => stats.cpu_nr_throttled = v.trim().parse().unwrap_or(0),
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
        stats.mem_swap_current = read_u64(base.join("memory.swap.current")).await.unwrap_or(0);

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
                "systemctl list-units --type=service --no-pager --no-legend --output=json 2>/dev/null || systemctl list-units --type=service --no-pager --no-legend --plain 2>/dev/null || rc-status --nocolor 2>/dev/null",
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
            load: String,
            active: String,
            sub: String,
        }

        let stdout = String::from_utf8_lossy(&out.stdout);
        if stdout.trim_start().starts_with('[') {
            let units: Vec<UnitJson> = serde_json::from_str(&stdout).unwrap_or_default();

            return Ok(units
                .into_iter()
                .map(|u| ServiceStatus {
                    name: u.unit,
                    state: u.active,
                    sub_state: u.sub,
                    enabled: u.load == "loaded",
                })
                .collect());
        }

        Ok(parse_services_text(&stdout))
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
                services.push(ServiceStatus {
                    name,
                    state: state.clone(),
                    sub_state: state,
                    enabled: true,
                });
            }
        }
        Ok(services)
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
            if matches!(fstype, "tmpfs" | "devtmpfs" | "proc" | "sysfs" | "devpts" | "cgroup2") {
                continue;
            }
            let total: u64 = parts[2].parse().unwrap_or(0);
            let used: u64 = parts[3].parse().unwrap_or(0);
            let avail: u64 = parts[4].parse().unwrap_or(0);
            let use_pct: f64 = parts[5]
                .trim_end_matches('%')
                .parse::<f64>()
                .unwrap_or(0.0);

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
                "-t", &init_pid.to_string(),
                "-m", "-p",  // mount and pid namespaces
                "--",
                "ps", "-eo", "pid,comm,pcpu,rss,state", "--no-headers", "--sort=-pcpu",
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
                return Some(ServiceStatus {
                    name,
                    state: state.clone(),
                    sub_state: state,
                    enabled: true,
                });
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 4 || !parts[0].ends_with(".service") {
                return None;
            }

            Some(ServiceStatus {
                name: parts[0].to_string(),
                state: parts[2].to_string(),
                sub_state: parts[3].to_string(),
                enabled: parts[1] == "loaded",
            })
        })
        .collect()
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
