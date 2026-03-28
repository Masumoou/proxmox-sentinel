# proxmox-sentinel

Lightweight agentless Proxmox monitoring daemon written in Rust.
Single binary, ~5-15 MB RAM, ≤1% CPU idle.

## What it collects

### From the Proxmox node itself (no VM agents needed)
| Source | Method | Data |
|--------|--------|------|
| Node CPU/mem/disk | Proxmox REST API | Usage, totals, load avg |
| Node uptime/version | REST API | PVE version, kernel |
| LXC CPU | `/sys/fs/cgroup/lxc/<id>/cpu.stat` | Usage µs, throttling |
| LXC memory | `/sys/fs/cgroup/lxc/<id>/memory.*` | RSS, anon, file, peak, limit |
| LXC I/O | `/sys/fs/cgroup/lxc/<id>/io.stat` | Read/write bytes + ops |
| LXC network | `/proc/<init_pid>/net/dev` | RX/TX bytes, packets, errors |
| LXC PID count | `/sys/fs/cgroup/lxc/<id>/pids.current` | Process count |
| LXC services | `pct exec <id> -- systemctl list-units` | Running services |
| LXC disk usage | `pct exec <id> -- df` | Per-mount usage % |
| LXC processes | `nsenter + ps` | Top CPU/mem consumers |
| **LXC logs** | Direct host read `/var/lib/lxc/<id>/rootfs/var/log/*` | **No SSH, no agent!** |
| KVM VM status | REST API | CPU, mem, disk I/O, net I/O |
| KVM VM network | QEMU Guest Agent | IP addresses |
| KVM VM filesystem | QEMU GA exec | df output |
| KVM VM services | SSH (key-based) | systemctl output |
| KVM VM logs | SSH tail | Apache, nginx, PHP-FPM etc |
| Storage | REST API | Used/total per pool (ZFS, LVM, NFS...) |
| Cluster status | REST API | Quorum, HA |

### Real-time log watching
- inotify-based (zero polling, zero CPU when quiet)
- Regex alert patterns with 5-minute deduplication
- Ring buffer: last 10,000 lines per source in memory

## Architecture

```
proxmox-sentinel (one binary on PVE node)
│
├── Task: API poller [every 15s]
│   └── Proxmox REST API → node + guest + storage metrics → Prometheus
│
├── Task: cgroup collector [every 5s, ~0% CPU]
│   ├── /sys/fs/cgroup/lxc/<id>/  → CPU, mem, I/O, pids
│   ├── /proc/<pid>/net/dev       → LXC network stats
│   └── Registers inotify log watchers for new LXCs
│
├── Task: inotify log watchers [event-driven, 0% CPU when quiet]
│   ├── /var/lib/lxc/<id>/rootfs/var/log/* (host-side, no agent!)
│   ├── /var/log/* (host logs)
│   └── Regex matching → alert channel
│
├── Task: VM collector [every 30s]
│   ├── QEMU Guest Agent → filesystem, processes, IPs
│   └── SSH → services, disk usage, log tail
│
├── Task: Alert dispatcher
│   ├── Deduplication (5-min silence window)
│   ├── Webhook (Alertmanager/Grafana OnCall/Slack)
│   └── Structured log output
│
└── HTTP server
    ├── GET /metrics   → Prometheus text format
    ├── GET /health    → "OK"
    └── GET /api/status → JSON
```

## Prometheus metrics exposed

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
pve_guest_memory_total_bytes{vmid,name,node,type}
pve_guest_network_in_bytes_total{vmid,name,node,type}
pve_guest_network_out_bytes_total{vmid,name,node,type}
pve_guest_disk_read_bytes_total{vmid,name,node,type}
pve_guest_disk_write_bytes_total{vmid,name,node,type}
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

## Build

```bash
# Requires Rust 1.78+
cargo build --release

# Strip debug symbols (optional, makes binary ~3x smaller)
strip target/release/proxmox-sentinel

# Cross-compile for Proxmox (Debian Bookworm, x86_64)
rustup target add x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

## Deploy

```bash
chmod +x deploy.sh
./deploy.sh root@your-proxmox-node
```

## Proxmox API token setup

In the Proxmox web UI:
1. **Datacenter → Permissions → API Tokens → Add**
   - User: `root@pam`
   - Token ID: `monitoring`
   - Privilege Separation: **unchecked** (inherits root perms)
   
   Or create a read-only token:
2. **Datacenter → Permissions → Roles → Add**
   - Name: `Sentinel`, privileges: `VM.Audit`, `Sys.Audit`, `Datastore.Audit`
3. Assign the role to your token user for path `/`

## SSH key setup for VM log collection

```bash
# On the Proxmox node
ssh-keygen -t ed25519 -f /root/.ssh/id_ed25519 -N ""

# Copy to each KVM VM
ssh-copy-id -i /root/.ssh/id_ed25519.pub root@<vm-ip>
```

## Grafana dashboard

Import the included `grafana-dashboard.json` or use PromQL:

```promql
# LXC memory usage %
100 * pve_lxc_cgroup_memory_current_bytes / pve_guest_memory_total_bytes

# Guests currently down
pve_guest_running == 0

# Top CPU guests
topk(5, pve_guest_cpu_usage_ratio * 100)

# Log alerts per hour
increase(pve_log_alert_total[1h])

# Storage fill rate
deriv(pve_storage_used_bytes[1h])
```

## Roadmap / what to add next

- [ ] **ZFS pool health** — `zpool status` parsing, scrub errors, degraded vdevs
- [ ] **Backup job status** — parse `/var/log/vzdump.log`, track last successful backup per VM
- [ ] **Certificate expiry** — check TLS cert expiry on monitored endpoints (your DC01 cert!)
- [ ] **Proxmox task log** — tail `/var/log/pve/tasks/active` for failed tasks
- [ ] **HA status** — cluster HA group monitoring via REST API
- [ ] **Replication status** — ZFS replication lag, failed replication jobs
- [ ] **DNS resolution checks** — verify internal DNS (PowerDNS) resolves expected names
- [ ] **HTTP endpoint probing** — HTTP GET checks with response time (like Blackbox Exporter)
- [ ] **SMART disk health** — drive temperature, reallocated sectors, pending sectors
- [ ] **Redis monitoring** — `INFO` command via direct TCP, no agent
- [ ] **PostgreSQL monitoring** — `pg_stat_*` via direct TCP socket
- [ ] **Web UI** — embedded single-page status dashboard (no Grafana required)
- [ ] **Push mode** — push metrics to remote Prometheus via remote_write
- [ ] **Ansible role** — automated deployment via your existing infra
