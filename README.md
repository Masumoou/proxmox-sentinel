<p align="center">
  <img src="https://img.shields.io/badge/rust-1.78+-orange?logo=rust" alt="Rust 1.78+">
  <img src="https://img.shields.io/badge/platform-linux%20x86__64-blue" alt="Linux x86_64">
  <img src="https://img.shields.io/badge/memory-5--15%20MB%20RSS-green" alt="Memory">
  <img src="https://img.shields.io/badge/CPU-≤1%25%20idle-green" alt="CPU">
  <img src="https://img.shields.io/github/actions/workflow/status/Masumoou/proxmox-sentinel/build.yml?label=build" alt="Build">
</p>

# ⚡ Proxmox Sentinel

**Lightweight, agentless Proxmox monitoring daemon with a real-time cyberpunk dashboard.**

Single binary. Zero agents. Zero dependencies on the target host. Deploys in 30 seconds.

Proxmox Sentinel runs directly on your PVE node, collecting deep metrics from LXC containers, KVM VMs, storage pools, and system logs — all without installing anything inside your guests. The embedded SvelteKit dashboard streams live data over WebSocket with a cyberpunk-inspired neon UI.

---

## 🖥️ Dashboard

The embedded web UI is compiled directly into the binary via `rust-embed` — no separate web server, no static files to manage. Access it at `http://your-node:9101/`.

**Features:**
- 🔴 Real-time WebSocket streaming — no polling, instant updates
- 🟢 Per-container service health with circular CPU/memory gauges  
- 🔵 Node overview bar with cluster-wide resource utilization
- 🟡 Live log viewer with severity-based filtering
- ⚫ Neon-bordered cards with cyberpunk dark theme

---

## 📊 What It Collects

### From the Proxmox node (no agents needed)

| Source | Method | Data |
|--------|--------|------|
| Node CPU/mem/disk | Proxmox REST API | Usage, totals, load avg |
| Node uptime/version | REST API | PVE version, kernel |
| LXC CPU | `/sys/fs/cgroup/lxc/<id>/cpu.stat` | Usage µs, throttling |
| LXC memory | `/sys/fs/cgroup/lxc/<id>/memory.*` | RSS, anon, file, peak, limit |
| LXC I/O | `/sys/fs/cgroup/lxc/<id>/io.stat` | Read/write bytes + ops |
| LXC network | `/proc/<init_pid>/net/dev` | RX/TX bytes, packets, errors |
| LXC PID count | `/sys/fs/cgroup/lxc/<id>/pids.current` | Process count |
| LXC services | `pct exec <id> -- systemctl` | Running services |
| LXC disk | `pct exec <id> -- df` | Per-mount usage % |
| **LXC logs** | Direct read: `/var/lib/lxc/<id>/rootfs/var/log/*` | **No SSH, no agent** |
| VM status | REST API | CPU, mem, disk I/O, net I/O |
| VM network | QEMU Guest Agent | IP addresses |
| VM filesystem | QEMU GA exec | df output |
| VM services | SSH (key-based) | systemctl output |
| VM logs | SSH tail | Apache, nginx, PHP-FPM etc |
| Storage | REST API | Used/total per pool (ZFS, LVM, NFS…) |

### Real-Time Log Watching
- **inotify-based** — zero polling, zero CPU when logs are quiet
- Regex alert patterns with 5-minute deduplication
- Ring buffer: last 10,000 lines per source in memory
- Live streaming to dashboard via WebSocket `log_line` events

---

## 🏗️ Architecture

```
proxmox-sentinel (single binary, ~5-15 MB RSS)
│
├── Task: API Poller [every 15s]
│   └── Proxmox REST API → node + guest + storage metrics
│
├── Task: cgroup Collector [every 5s, ~0% CPU]
│   ├── /sys/fs/cgroup/lxc/<id>/  → CPU, mem, I/O, pids
│   ├── /proc/<pid>/net/dev       → network stats
│   └── Registers inotify watchers for new LXCs
│
├── Task: inotify Log Watchers [event-driven]
│   ├── /var/lib/lxc/<id>/rootfs/var/log/* (host-side reads)
│   ├── /var/log/* (host logs)
│   └── Pattern matching → alert channel + WebSocket broadcast
│
├── Task: VM Collector [every 30s]
│   ├── QEMU Guest Agent → filesystem, processes, IPs
│   └── SSH fallback → services, disk usage, logs
│
├── Task: Alert Dispatcher
│   ├── Deduplication (5-min silence window)
│   ├── Webhook (Alertmanager / Grafana OnCall / Slack)
│   └── Structured log output
│
└── HTTP + WebSocket Server (:9101)
    ├── GET /metrics    → Prometheus text format
    ├── GET /health     → "OK"
    ├── GET /ws         → WebSocket (cluster_update, lxc_detail, vm_detail, log_line)
    └── GET /*          → Embedded SvelteKit dashboard (rust-embed)
```

---

## 🔧 Build

```bash
# Requires Rust 1.78+
cargo build --release

# The frontend must be built first (compiled into the binary)
cd frontend && npm ci && npm run build && cd ..
cargo build --release

# Cross-compile for Proxmox (Debian Bookworm, x86_64)
rustup target add x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

The GitHub Actions CI builds automatically on every push to `main`.

---

## 🚀 Deploy

```bash
# One-command deploy
chmod +x deploy.sh
./deploy.sh root@your-proxmox-node

# Or manually
scp target/release/proxmox-sentinel root@pve:/usr/local/bin/
scp config.toml.example root@pve:/etc/proxmox-sentinel/config.toml
# Edit config, then:
systemctl enable --now proxmox-sentinel
```

---

## ⚙️ Configuration

Copy `config.toml.example` to `/etc/proxmox-sentinel/config.toml` and edit:

```toml
[proxmox]
api_url          = "https://10.10.207.1:8006"
api_token_id     = "sentinel@pam!monitoring"
api_token_secret = "your-token-here"
insecure_tls     = true

[metrics]
listen_addr = "0.0.0.0"
listen_port = 9101
```

### Proxmox API Token Setup

1. **Datacenter → Permissions → API Tokens → Add**
   - User: `root@pam`, Token ID: `monitoring`
   - Privilege Separation: **unchecked**

   Or create a read-only role:
2. **Datacenter → Permissions → Roles → Add**
   - Name: `Sentinel`, privileges: `VM.Audit`, `Sys.Audit`, `Datastore.Audit`

### SSH Key Setup (for VM log collection)

```bash
ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N ""
ssh-copy-id -i /root/.ssh/id_ed25519.pub root@<vm-ip>
```

---

## 📈 Prometheus Metrics

All metrics are exposed at `GET :9101/metrics`:

```
# Node
pve_node_cpu_usage_ratio{node}
pve_node_memory_used_bytes{node}
pve_node_memory_total_bytes{node}
pve_node_rootfs_used_bytes{node}
pve_node_rootfs_total_bytes{node}
pve_node_load_average{node,interval="1|5|15"}
pve_node_uptime_seconds{node}

# Guests (VM + LXC)
pve_guest_running{vmid,name,node,type}
pve_guest_cpu_usage_ratio{vmid,name,node,type,status}
pve_guest_memory_used_bytes{vmid,name,node,type}
pve_guest_uptime_seconds{vmid,name,node,type}

# LXC cgroup detail
pve_lxc_cgroup_memory_current_bytes{vmid,name}
pve_lxc_cgroup_memory_anon_bytes{vmid,name}
pve_lxc_cgroup_cpu_throttled_total{vmid,name}
pve_lxc_cgroup_pid_count{vmid,name}

# Storage
pve_storage_used_bytes{storage,node,type}
pve_storage_total_bytes{storage,node,type}
pve_storage_avail_bytes{storage,node,type}

# Log alerts
pve_log_alert_total{source,pattern,severity}
```

---

## 🗺️ Roadmap

- [ ] ZFS pool health — `zpool status` parsing, scrub errors, degraded vdevs
- [ ] Backup job status — parse vzdump logs, track last successful backup per VM
- [ ] Certificate expiry — TLS cert monitoring on endpoints
- [ ] HA status — cluster HA group monitoring via REST API
- [ ] Replication status — ZFS replication lag tracking
- [ ] SMART disk health — drive temperature, reallocated sectors
- [ ] Push mode — remote_write to external Prometheus

---

## 📄 License

MIT
