# Proxmox Sentinel

Proxmox Sentinel is a self-hosted monitoring daemon for Proxmox VE.

It runs on a Proxmox node, talks to the Proxmox API, reads local host data, and serves an embedded dashboard, Prometheus metrics, WebSocket live updates, and alert events from one Rust binary.

The project is designed for Proxmox users who want a simple install, useful visibility without Grafana on day one, and no cloud dependency.

## What It Does

- Monitors Proxmox nodes, VMs, LXC containers, storage, backups, tasks, logs, security posture, certificates, ZFS, LVM-thin, Ceph, HAProxy, and application health.
- Provides a built-in dashboard on port `9101` by default.
- Exposes Prometheus metrics at `/metrics`.
- Streams live updates over `/ws`.
- Stores metric and alert history locally in SQLite.
- Sends alerts to a configured webhook endpoint.
- Ships as a single Linux binary and as a Debian package.
- Includes `proxmox-sentinel init` for first-time setup.
- Includes `proxmox-sentinel doctor` for installation and visibility checks.

## Important Visibility Rules

Sentinel can see different levels of detail depending on guest type.

| Target | What works without guest install | What needs guest access |
|---|---|---|
| Proxmox node | CPU, RAM, load, storage, services, logs, ZFS, LVM, Ceph, tasks, HA, security checks | Nothing |
| LXC container | Status, CPU, RAM, cgroups, PID count, disk mounts, services via `pct exec`, rootfs logs, OS from `/etc/os-release` | Nothing extra in most cases |
| QEMU/KVM VM | Proxmox-level status, CPU, RAM, disk size, running/stopped state | IP address, OS version, services, filesystem mounts, process details |

For KVM VMs, install and enable QEMU Guest Agent or configure SSH in Sentinel. Without one of those, Proxmox itself cannot tell Sentinel which services are running inside the VM.

## Dashboard Pages

| Page | Purpose |
|---|---|
| Overview | Cluster summary, guest cards, live telemetry, alerts |
| Nodes | Node CPU, RAM, status, and 24h performance charts |
| VMs / Guests | Global VM and LXC inventory with status, node, CPU, memory, IP, OS, services, agent state |
| Containers | LXC-focused view with cgroup and service visibility |
| Storage | Proxmox storage, ZFS pools, LVM-thin pools, Ceph status |
| Backups | Last backup per guest, backup age, failed backup history, guests without backups |
| Tasks | Proxmox task history, failed tasks, long-running tasks |
| Logs | Live host, Proxmox, VM, LXC, and app log stream where configured |
| Alerts | Recent alert history |
| Security | Read-only posture checks and certificate status |
| HAProxy | HAProxy frontend/backend/server health from stats CSV |
| File Activity | Access-log activity and request/security events |
| App Overview | Application metrics and application logs |
| Settings | Runtime configuration and project info |

## Install

### Option 1: One-Line Installer

```bash
curl -fsSL https://raw.githubusercontent.com/Masumoou/proxmox-sentinel/main/install.sh | bash
```

The installer downloads the latest release. If a Debian package exists, it uses that. Otherwise it falls back to the release binary.

### Option 2: Debian Package

```bash
wget https://github.com/Masumoou/proxmox-sentinel/releases/latest/download/proxmox-sentinel.deb
dpkg -i proxmox-sentinel.deb
```

### Option 3: Binary

```bash
wget https://github.com/Masumoou/proxmox-sentinel/releases/latest/download/proxmox-sentinel-linux-amd64 \
  -O /usr/local/bin/proxmox-sentinel

chmod +x /usr/local/bin/proxmox-sentinel
```

## Create A Read-Only Proxmox Token

Run this on a Proxmox node:

```bash
pveum role add SentinelAudit -privs "VM.Audit Sys.Audit Datastore.Audit" 2>/dev/null || true
pveum user add sentinel@pve --comment "Proxmox Sentinel monitoring" 2>/dev/null || true
pveum aclmod / -user sentinel@pve -role SentinelAudit
pveum user token add sentinel@pve monitoring --privsep 0
```

Save the token value. Proxmox only shows it once.

Example token id:

```text
sentinel@pve!monitoring
```

## First-Time Setup

Run the interactive setup:

```bash
proxmox-sentinel init
```

It asks for:

- Proxmox API URL
- API token ID
- API token secret
- listen port
- TLS verification yes/no
- dashboard auth yes/no
- Prometheus endpoint yes/no

It generates:

- `/etc/proxmox-sentinel/config.toml`
- `/etc/systemd/system/proxmox-sentinel.service`

Start the service:

```bash
systemctl daemon-reload
systemctl enable --now proxmox-sentinel
systemctl status proxmox-sentinel --no-pager
```

Open the dashboard:

```text
http://YOUR-PROXMOX-IP:9101
```

Health checks:

```bash
curl http://127.0.0.1:9101/health
curl http://127.0.0.1:9101/api/status
curl http://127.0.0.1:9101/metrics | head
```

## Doctor

Run:

```bash
proxmox-sentinel --config /etc/proxmox-sentinel/config.toml doctor
```

Doctor checks:

- Config file is valid
- Proxmox API connection works
- Nodes can be listed
- Guests can be listed
- cgroup filesystem is readable
- LXC rootfs log paths are accessible
- configured port can be bound
- systemd service is installed

Use this after install, after upgrades, and when the dashboard shows missing data.

## Configuration

Main config path:

```text
/etc/proxmox-sentinel/config.toml
```

Example:

```toml
[proxmox]
api_url = "https://10.10.207.13:8006"
api_token_id = "sentinel@pve!monitoring"
api_token_secret = "replace-with-token-secret"
nodes = []
insecure_tls = true

[metrics]
listen_addr = "0.0.0.0"
listen_port = 9101
auth = ""
prometheus_enabled = true

[alerts]
enabled = true
webhook_url = ""
cpu_threshold = 90.0
memory_threshold = 85.0
disk_threshold = 90.0

[ssh]
private_key_path = "/root/.ssh/id_ed25519"
user = "root"
timeout_secs = 10
skip_vmids = []

[platform]
enabled = true
interval_secs = 60
backup_warning_hours = 48
backup_critical_hours = 72
task_long_running_minutes = 60
snapshot_warning_days = 7
snapshot_max_count = 5
zfs_usage_warning_pct = 80.0
security_checks = true

[certificates]
enabled = true
warning_days = 30
critical_days = 7

[[certificates.targets]]
name = "proxmox-ui"
url = "https://pve.example.com:8006"
```

### Dashboard Auth

Set basic auth with:

```toml
[metrics]
auth = "admin:change-this-password"
```

The dashboard and API require auth when this is set. WebSocket handling is built to work with the dashboard session.

### Webhook Alerts

Set:

```toml
[alerts]
enabled = true
webhook_url = "https://your-webhook-endpoint.example"
```

Test:

```bash
curl -X POST http://127.0.0.1:9101/api/v1/alerts/test
```

Webhook payloads are structured so they can be consumed by Alertmanager-compatible receivers or custom webhook handlers.

## What Sentinel Collects

### Proxmox API

Collected through the Proxmox REST API:

- node list
- node status
- node CPU, memory, swap, load, uptime
- node Proxmox version
- QEMU VM list
- LXC list
- guest status
- guest CPU and memory usage from Proxmox
- Proxmox storage status
- task history
- backup-related tasks

### LXC Containers

Collected from the Proxmox host:

- cgroup v2 CPU stats
- cgroup memory stats
- I/O stats
- PID count
- service list through `pct exec <vmid> -- systemctl`
- disk mounts through `pct exec <vmid> -- df`
- OS and OS version from container `/etc/os-release`
- log files from `/var/lib/lxc/<vmid>/rootfs/var/log`

This is why Sentinel runs as root on the Proxmox node.

### QEMU/KVM VMs

Always available from Proxmox:

- VMID
- name
- node
- status
- CPU usage reported by Proxmox
- memory usage reported by Proxmox
- configured disk size
- template flag where available

Available when QEMU Guest Agent responds:

- IP addresses
- OS information where exposed
- filesystem/mount details
- guest command execution for service and process checks

Available when SSH fallback is configured:

- services from `systemctl`
- OS and version from `/etc/os-release`
- disk mounts from `df`
- selected logs

For best VM visibility, install QEMU Guest Agent in every Linux VM:

```bash
apt update
apt install -y qemu-guest-agent
systemctl enable --now qemu-guest-agent
```

Then enable guest agent for the VM in Proxmox:

```bash
qm set VMID --agent enabled=1
```

Fedora:

```bash
dnf install -y qemu-guest-agent
systemctl enable --now qemu-guest-agent
```

### ZFS

Collected locally with ZFS tools when available:

- pool name
- pool state: `ONLINE`, `DEGRADED`, `FAULTED`, etc.
- used capacity
- fragmentation
- scrub status
- scrub errors
- read/write/checksum error signals
- vdev/device problem text from `zpool status`

Alerts include degraded pools, scrub errors, high usage, checksum errors, and missing or unavailable pools.

### Backups

Collected from Proxmox task history:

- backup tasks
- last successful backup per guest
- failed backup tasks
- backup age
- guests with no recent backup
- warning and critical backup age thresholds

PBS-specific API support can be layered on top of this, but the current release already detects backup health from Proxmox-visible task history.

### Tasks

Collected from Proxmox task history:

- failed tasks
- running tasks
- long-running tasks
- backup, migration, clone, restore, snapshot, and disk move task types
- task status and user

Alerts include failed tasks and tasks running longer than the configured threshold.

### HA And Cluster

Collected locally when the node is part of a cluster:

- quorum status from `pvecm status`
- cluster membership summary
- Corosync-related status where available
- HA resource state from `ha-manager status --verbose`
- node membership changes visible through cluster status

For single-node Proxmox installs, the dashboard can hide or downplay cluster-only data.

### Certificates

Collected from:

- local Proxmox web certificate when available
- configured HTTPS targets in `[[certificates.targets]]`

Checks include:

- expiry date
- days remaining
- expired certificates
- warning and critical expiry thresholds
- endpoint reachability

### Security Checks

Security checks are read-only. Sentinel reports posture; it does not auto-fix.

Checks include:

- root SSH login setting
- Proxmox version visibility
- enterprise/no-subscription repository posture
- firewall status
- guest agent visibility posture
- API/token posture where available from readable config/API data
- old snapshots and templates where visible

### Snapshots

Collected with Proxmox tooling:

- snapshot count per guest
- snapshot age where available
- old snapshots
- snapshot descriptions where available

Alerts include old snapshots and too many snapshots on one guest.

### Storage

Collected from Proxmox API and local tools:

- directory storage
- NFS storage
- LVM
- LVM-thin data usage
- LVM-thin metadata usage
- ZFS
- Ceph status where available
- enabled/disabled storage state
- available/used/total space

Alerts include storage unavailable, high usage, LVM-thin metadata pressure, Ceph health warnings, and NFS availability problems where detected.

### Ceph

If Ceph is installed and `ceph status --format json` works:

- health status
- monitor quorum
- OSD up/down count
- pool/PG summary
- degraded PG indicators
- nearfull/full health text
- recovery status text where available

### HAProxy

When enabled:

```toml
[haproxy]
enabled = true
stats_url = "http://127.0.0.1:8404/stats;csv"
interval_secs = 10
```

Sentinel collects:

- proxy status
- backend/server status
- current sessions
- bytes in/out
- HTTP 5xx counts
- backend down alerts

### File Activity

When enabled:

```toml
[file_activity]
enabled = true
watch_paths = ["/var/log/nginx/access.log"]
access_log_regex = "..."
```

Sentinel tails access logs and emits security/activity events such as:

- IP
- user
- method
- path
- status code
- response size
- failed request count
- most active IP

### Application Metrics

Use `[[app_metrics]]` for app-level checks:

```toml
[[app_metrics]]
enabled = true
name = "nextcloud"
kind = "http_json"
endpoint_url = "https://nextcloud.example.com/status.php"
interval_secs = 60
```

Supported styles include:

- `nextcloud_occ`
- `http_json`
- `shell_json`

Mapped values are exposed as Prometheus metric `pve_app_metric` and shown in App Overview.

### Application Logs

Use `[[app_logs]]` for app log parsing:

```toml
[[app_logs]]
enabled = true
name = "nginx"
log_file_path = "/var/log/nginx/access.log"
log_format = "nginx_combined"
slow_request_threshold_ms = 1000
```

Supported formats include:

- `nextcloud_json`
- `nginx_combined`
- `apache_combined`

## Alerts

Built-in alert categories include:

- node high CPU
- node high memory
- node high disk
- guest down
- guest high CPU
- guest high memory
- storage unavailable
- disk full
- service unavailable
- HAProxy backend down
- PostgreSQL down
- Redis down
- object storage degraded
- app down
- app high error rate
- app authentication failures
- VM migration detected
- VM connection lost
- OOM killed
- ZFS degraded or unhealthy
- backup missing or failed
- failed Proxmox task
- long-running task
- quorum or HA issue
- certificate expiry
- security posture warning
- snapshot age/count warning
- Ceph health issue
- LVM-thin pressure

Thresholds are configured in `config.toml`. Per-guest and per-service custom rules are part of the alert-rule model and should be added to config as the rule schema grows.

## Prometheus

Metrics endpoint:

```text
http://YOUR-PROXMOX-IP:9101/metrics
```

Common metrics include:

```text
pve_node_cpu_usage_ratio
pve_node_memory_used_bytes
pve_node_memory_total_bytes
pve_node_rootfs_used_bytes
pve_node_swap_used_bytes
pve_guest_cpu_usage_ratio
pve_guest_memory_used_bytes
pve_guest_running
pve_storage_used_bytes
pve_storage_total_bytes
pve_lxc_cgroup_memory_current_bytes
pve_lxc_cgroup_pid_count
haproxy_server_up
pve_postgres_up
pve_redis_up
pve_object_storage_up
pve_app_metric
pve_oom_kill_total
```

## Deployment Modes

### Mode A: Single-Node / Cluster API Mode

Install Sentinel on one Proxmox node.

It talks to the Proxmox API and monitors the cluster from that node. This is best for small labs and simple clusters.

### Mode B: Agent / Hub Mode

Install Sentinel on every Proxmox node as an agent.

One Sentinel server receives events from agents and serves the central dashboard. This gives better local visibility for host-specific checks such as logs, cgroups, ZFS, SMART-style checks, local services, and node-local files.

Config:

```toml
[cluster]
mode = "agent" # standalone | agent | server
server_url = "http://sentinel-hub.example:9101"
shared_secret = "change-me"
```

## Release Artifacts

GitHub releases include:

- `proxmox-sentinel-linux-amd64`
- `proxmox-sentinel-linux-x86_64`
- `proxmox-sentinel.deb`
- `checksums.txt`
- `proxmox-sentinel.service`
- `config.toml.example.release`

Verify checksums:

```bash
sha256sum -c checksums.txt
```

## Upgrade

Debian package:

```bash
wget https://github.com/Masumoou/proxmox-sentinel/releases/latest/download/proxmox-sentinel.deb
dpkg -i proxmox-sentinel.deb
systemctl restart proxmox-sentinel
```

Binary:

```bash
systemctl stop proxmox-sentinel
wget https://github.com/Masumoou/proxmox-sentinel/releases/latest/download/proxmox-sentinel-linux-amd64 \
  -O /usr/local/bin/proxmox-sentinel
chmod +x /usr/local/bin/proxmox-sentinel
systemctl start proxmox-sentinel
```

Run doctor after upgrade:

```bash
proxmox-sentinel --config /etc/proxmox-sentinel/config.toml doctor
```

## Troubleshooting

### Dashboard opens but data is empty

Check the service:

```bash
systemctl status proxmox-sentinel --no-pager
journalctl -u proxmox-sentinel -n 100 --no-pager
curl http://127.0.0.1:9101/health
```

Run:

```bash
proxmox-sentinel --config /etc/proxmox-sentinel/config.toml doctor
```

### VM services show 0

For QEMU/KVM VMs this means Sentinel cannot see inside the guest yet.

Fix one of these:

- install and enable QEMU Guest Agent in the VM
- enable the guest agent option in Proxmox with `qm set VMID --agent enabled=1`
- configure SSH fallback in `[ssh]`

For LXC containers, services should work through `pct exec` unless the container blocks systemd access or the host lacks permission.

### OS shows unknown

For LXC, Sentinel reads `/etc/os-release` from the container rootfs.

For QEMU/KVM VMs, real OS/version needs QEMU Guest Agent or SSH. Without that, Sentinel can only infer a best-effort OS from VM name, tags, or config metadata.

### IP address missing

For QEMU/KVM VMs, IP detection needs QEMU Guest Agent or SSH. Proxmox API alone does not reliably expose guest IP addresses.

### HAProxy page has no data

Enable HAProxy stats CSV and configure:

```toml
[haproxy]
enabled = true
stats_url = "http://YOUR-HAPROXY-IP:8404/stats;csv"
```

Then restart:

```bash
systemctl restart proxmox-sentinel
```

### Storage page waits for data

Check:

```bash
pvesm status
pvesh get /nodes/$(hostname)/storage
journalctl -u proxmox-sentinel -n 100 --no-pager
```

Guest disk mounts need LXC access, QEMU Guest Agent, or SSH.

### Webhook errors

If you do not use webhooks, leave `webhook_url` empty or remove it.

If you do use webhooks, test the endpoint manually and then use:

```bash
curl -X POST http://127.0.0.1:9101/api/v1/alerts/test
```

## Build From Source

Requirements:

- Rust stable
- Node.js 22+
- npm
- Linux target for release builds

Build:

```bash
cd frontend
npm ci
npm run build
cd ..
cargo build --release
```

Cross-build target used by CI:

```bash
rustup target add x86_64-unknown-linux-gnu
cargo build --release --target x86_64-unknown-linux-gnu
```

## Privacy

Sentinel is privacy-first:

- no cloud dependency
- no telemetry by default
- no data leaves the node unless you configure webhooks or hub/agent forwarding
- credentials are stored locally in `/etc/proxmox-sentinel/config.toml`
- read-only Proxmox API token is recommended
- SQLite data stays on the host by default

## License

MIT
