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

## What Sentinel Is Best At

Sentinel focuses on Proxmox operational health, not only raw CPU/RAM graphs.

It helps answer:

- Which VMs or LXCs have stale or missing backups?
- Are ZFS, LVM-thin, Ceph, NFS, and Proxmox storage pools healthy?
- Are snapshots growing old or piling up?
- Did a Proxmox backup, migration, restore, clone, or snapshot task fail?
- Are QEMU Guest Agents responding so IPs, OS details, filesystems, and services are visible?
- Are certificates close to expiry?
- Are logs showing OOM, disk, SSH, database, PHP, web server, or application errors?

Core collectors focus on Proxmox, guests, storage, backups, tasks, snapshots, ZFS/Ceph/LVM, alerts, dashboard, and Prometheus. Postgres, Redis, HAProxy, object storage, app metrics, and app logs are optional integrations.

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
It verifies release checksums when `checksums.txt` is available and preserves an existing `/etc/proxmox-sentinel/config.toml` during upgrades.

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
pveum role add SentinelAudit -privs "VM.Audit VM.GuestAgent.Audit VM.GuestAgent.Unrestricted Sys.Audit Datastore.Audit Pool.Audit" 2>/dev/null || true
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
- QEMU Guest Agent API method and permissions are sane when a running VM with an agent is available
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
ignore_template_guests = true

[ssh]
private_key_path = "/root/.ssh/id_ed25519"
user = "root"
timeout_secs = 10
skip_vmids = []

[platform]
enabled = true
interval_secs = 60
backup_warn_hours = 48
backup_critical_hours = 72
task_long_running_minutes = 60
snapshot_warn_days = 7
snapshot_max_count = 5
zfs_usage_threshold = 80.0
lvmthin_data_warn_pct = 85.0
lvmthin_data_critical_pct = 95.0
lvmthin_metadata_warn_pct = 75.0
lvmthin_metadata_critical_pct = 90.0
security_enabled = true
exclude_backup_vmids = [9000, 9001]
exclude_guest_agent_vmids = []
exclude_snapshot_vmids = []
ignore_templates = true
ignore_stopped_guests_for_backup = true

[backup_policy]
enabled = true
default_required = true
ignore_stopped_guests = true
ignore_templates = true
warn_hours = 48
critical_hours = 72
exclude_vmids = [9000, 9001]
include_tags = []
exclude_tags = ["nobackup", "test", "template"]

[[backup_policy.tag_rules]]
tag = "critical"
warn_hours = 24
critical_hours = 36
required = true

[[backup_policy.tag_rules]]
tag = "daily-backup"
warn_hours = 36
critical_hours = 48
required = true

[[alert_rules]]
name = "web01-cpu-high"
target = "vm"
vmid = 101
metric = "cpu"
operator = ">"
threshold = 86
duration_secs = 120
severity = "warning"

[[alert_rules]]
name = "web01-nginx-down"
target = "service"
vmid = 101
service = "nginx"
condition = "down"
duration_secs = 60
severity = "critical"

[certificates]
warn_days = 30
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

Do not expose Sentinel directly to the internet. Use a VPN, WireGuard, Tailscale, or a TLS reverse proxy with authentication. Sentinel logs a startup warning when it is bound to `0.0.0.0` without dashboard auth.

### Minimum Proxmox API Permissions

Use a read-only token instead of an Administrator token. Recommended minimum privileges:

```text
Sys.Audit
VM.Audit
VM.GuestAgent.Audit          # native guest-agent reads: ping, OS, IP, filesystem info
VM.GuestAgent.Unrestricted   # guest exec for service/process discovery
Datastore.Audit
Pool.Audit
SDN.Audit      # only if SDN features are used
```

Sentinel uses Proxmox's method map correctly: `POST` for `/agent/ping` and `/agent/exec`, and `GET` for `/agent/get-osinfo`, `/agent/network-get-interfaces`, `/agent/get-fsinfo`, and `/agent/exec-status`.

Guest exec uses the Proxmox-supported JSON array form:

```json
{"command":["/bin/sh","-lc","(systemctl list-units --type=service --all --no-pager --no-legend --plain; systemctl list-units --type=service --state=running --no-pager --no-legend --plain; systemctl --failed --type=service --no-pager --no-legend --plain)"]}
```

If guest exec fails, Sentinel keeps `agent=true` when ping/native endpoints still work, keeps showing OS/IP/mounts, logs `guest-agent exec_error`, and reports zero services until the exec permission or command issue is fixed.

Service discovery collects all units reported by the guest, not a hardcoded list. Each service keeps its name, load state, active state, sub-state, description, running/failed flags, display classification, and listening ports when `ss -lntup` is available. Classification is only used to sort the compact dashboard preview so failed services and application services such as `apache2`, `php8.3-fpm`, `postgresql`, `redis`, `haproxy`, and `ssh` appear before noisy system units. The full guest detail table keeps every discovered service.

The daemon still runs as root on the Proxmox host for local cgroup, LXC, and log access.

### Custom Alert Rules

Custom alert rules are configured with `[[alert_rules]]`. They are evaluated against live Proxmox API data, guest detail data, storage data, and discovered service state.

Supported targets:

```text
node
vm
lxc
guest
service
storage
```

Supported operators:

```text
>
>=
<
<=
==
=
!=
```

`duration_secs` means the condition must stay true for the configured duration before Sentinel fires the alert. If the condition becomes false, the timer resets.

Examples:

```toml
[[alert_rules]]
name = "vm-101-cpu-high"
target = "vm"
vmid = 101
metric = "cpu"
operator = ">"
threshold = 86
duration_secs = 120
severity = "warning"

[[alert_rules]]
name = "vm-101-stopped"
target = "vm"
vmid = 101
metric = "status"
operator = "=="
value = "stopped"
duration_secs = 60
severity = "critical"

[[alert_rules]]
name = "vm-101-nginx-down"
target = "service"
vmid = 101
service = "nginx"
condition = "down"
duration_secs = 60
severity = "critical"

[[alert_rules]]
name = "pve-node-memory-high"
target = "node"
node = "pve-01"
metric = "memory"
operator = ">"
threshold = 90
duration_secs = 300
severity = "warning"

[[alert_rules]]
name = "local-lvm-usage-high"
target = "storage"
node = "pve-01"
storage = "local-lvm"
metric = "usage"
operator = ">"
threshold = 85
duration_secs = 300
severity = "warning"
```

Service rules support these conditions:

```text
down
not_running
failed
inactive
dead
missing
running
```

`down` remains backward compatible and means the service is missing or not running. Service rules can target any discovered service, for example `php8.3-fpm` on one VM:

```toml
[[alert_rules]]
name = "vm-104-php-fpm-down"
target = "service"
vmid = 104
service = "php8.3-fpm"
condition = "down"
duration_secs = 60
severity = "critical"
```

### Backup Policy

Backup policy controls which guests require fresh backup artifacts. Sentinel checks real Proxmox storage content and local vzdump files, then applies policy to avoid false positives.

Important fields:

```toml
[backup_policy]
enabled = true
default_required = true
ignore_stopped_guests = true
ignore_templates = true
warn_hours = 48
critical_hours = 72
exclude_vmids = [9000, 9001]
include_tags = []
exclude_tags = ["nobackup", "test", "template"]
```

Tag rules override the default backup window:

```toml
[[backup_policy.tag_rules]]
tag = "critical"
warn_hours = 24
critical_hours = 36
required = true

[[backup_policy.tag_rules]]
tag = "daily-backup"
warn_hours = 36
critical_hours = 48
required = true

[[backup_policy.tag_rules]]
tag = "nobackup"
warn_hours = 48
critical_hours = 72
required = false
```

Practical behavior:

- A VM tagged `critical` must have a newer backup than the shorter critical window.
- A VM tagged `daily-backup` gets its own daily window.
- A VM tagged `nobackup`, `test`, or `template` is ignored by default.
- Templates and stopped guests are ignored by default to reduce noisy alerts.

### Webhook Alerts

Set:

```toml
[alerts]
enabled = true
webhook_url = "https://your-webhook-endpoint.example"
ignore_template_guests = true
```

Test:

```bash
curl -X POST http://127.0.0.1:9101/api/v1/alerts/test
```

Webhook payloads are structured so they can be consumed by Alertmanager-compatible receivers or custom webhook handlers.

Implemented alert delivery today:

- generic webhook
- Alertmanager-compatible webhook payloads

Roadmap alert channels, not yet implemented as first-class providers:

- Telegram
- Discord
- Slack
- Email SMTP
- Gotify
- ntfy.sh
- PagerDuty
- Opsgenie
- Microsoft Teams
- Grafana OnCall direct integration

Some of those tools can already receive alerts if they expose a compatible webhook endpoint, but Sentinel currently treats them as generic webhooks.

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
- full service inventory through `pct exec <vmid> -- systemctl`, including all, running, and failed service views
- listening ports from `ss -lntup` when available
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
- guest command execution for full service inventory, failed service checks, listening ports, and process checks

Available when SSH fallback is configured:

- full service inventory from `systemctl`
- listening ports from `ss -lntup`
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

Collected from Proxmox storage content APIs, local `vzdump` backup artifacts, and task history:

- backup tasks
- real `vzdump-*` backup artifacts on Proxmox backup-capable storage
- last successful backup per guest
- failed backup tasks
- backup age
- guests with no recent backup
- warning and critical backup age thresholds

Backup monitoring is policy-driven to reduce false positives:

- `exclude_vmids` skips guests that should never alert
- `exclude_tags = ["nobackup", "test", "template"]` skips tagged guests
- `ignore_stopped_guests` and `ignore_templates` are enabled by default
- tag rules such as `critical` and `daily-backup` can use stricter backup windows

Task history is still used for failed or long-running backup jobs, but freshness is based on actual backup files when Proxmox exposes them.

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
- snapshot and guest visibility signals where available

### Snapshots

Collected with Proxmox API snapshot metadata:

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
json_path_mappings = [
  { json_path = "installed", metric_name = "installed", metric_type = "gauge", label = "Installed", unit = "" },
  { json_path = "maintenance", metric_name = "maintenance_mode", metric_type = "gauge", label = "Maintenance", unit = "" }
]
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
target_vmid = 100
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

Thresholds are configured in `config.toml`.

Custom alert rules support per-node, per-VM, per-LXC, per-storage, and per-service checks:

```toml
[[alert_rules]]
name = "postgres-vm-memory-high"
target = "vm"
vmid = 205
metric = "memory"
operator = ">"
threshold = 80
duration_secs = 180
severity = "critical"

[[alert_rules]]
name = "database-vm-down"
target = "vm"
vmid = 205
metric = "status"
operator = "=="
value = "stopped"
duration_secs = 60
severity = "critical"

[[alert_rules]]
name = "web01-nginx-down"
target = "service"
vmid = 101
service = "nginx"
condition = "down"
duration_secs = 60
severity = "critical"
```

Rules use duration tracking so brief spikes do not immediately page you. Alert deduplication is shared across all collector tasks.

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
- `proxmox-sentinel.deb`
- `checksums.txt`
- `proxmox-sentinel.service`
- `config.toml.example.release`

Verify checksums:

```bash
sha256sum -c checksums.txt
```

Release automation is handled by `.github/workflows/release.yml` and runs when a version tag such as `v0.3.0-beta` is pushed. Normal branch and pull request checks run through CI/build workflows without publishing a release.

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

For release validation across real Proxmox environments, use [docs/Real-World-Testing.md](docs/Real-World-Testing.md).

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
- `pkg-config` and `libssl-dev` on Debian/Proxmox hosts
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
