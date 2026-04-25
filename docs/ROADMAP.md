# Proxmox Sentinel Roadmap

This roadmap captures the product direction for turning Proxmox Sentinel into a complete, global Proxmox monitoring tool.

## Install And First Run

- Add `proxmox-sentinel init`.
- Prompt for Proxmox API URL, API token ID, API token secret, listen port, TLS verification, dashboard auth, and Prometheus endpoint.
- Generate `/etc/proxmox-sentinel/config.toml`.
- Generate `/etc/systemd/system/proxmox-sentinel.service`.
- Add `proxmox-sentinel doctor`.
- Doctor checks: Proxmox API connectivity, node listing, guest listing, cgroup access, LXC rootfs log access, port binding, config validity, and systemd service installation.

## Debian Package And Installer

- Support one-line installer via `curl -fsSL .../install.sh | bash`.
- Publish Debian package for Proxmox VE nodes.
- Release artifacts should include:
  - `proxmox-sentinel-linux-amd64`
  - `proxmox-sentinel.deb`
  - `checksums.txt`
  - systemd service file
  - `config.toml.example`

## ZFS Health

- Monitor `zpool status`.
- Track pool state, scrub status, scrub errors, vdev errors, read/write/checksum errors, capacity, and fragmentation.
- Alerts: pool degraded, scrub errors, high usage, checksum errors, pool not imported.
- Dashboard panel: ZFS Pools.

## Backup Monitoring

- Monitor vzdump jobs and Proxmox Backup Server jobs.
- Track last successful backup per VM/LXC, backup age, duration, size, failed backup logs, VMs without backups, and backup storage usage.
- Alerts: no recent backup, backup failed, backup storage almost full, backup job stuck, PBS datastore unavailable.

## Proxmox Task History

- Track failed tasks and long-running tasks.
- Cover backup, migration, clone, restore, snapshot, and disk move tasks.
- Alerts: migration failed, backup failed, snapshot failed, restore failed, task running too long.

## HA And Cluster Quorum

- Monitor cluster quorum, node membership, corosync status, HA group health, HA resource state, fencing events, and node offline events.
- Alerts: quorum lost, node left cluster, HA resource failed, corosync unstable, VM restarted by HA.
- Hide cluster-only sections automatically for single-node users.

## Certificate Expiry

- Monitor Proxmox UI certificates, configured HTTPS endpoints, VM services, and custom endpoints.
- Alerts: certificate expires in 30 days, 7 days, expired, hostname mismatch.

## Security Posture

- Optional read-only checks for root login, 2FA status, API tokens without expiration, privileged API tokens, old Proxmox versions, repository setup, firewall state, VM protection flags, guest agent state, unused snapshots, and old templates.
- Do not auto-fix by default.
- Dashboard panel: Security Checks.

## Inventory

- Add a complete VM/LXC inventory page.
- Show VMID, name, type, node, status, CPU, RAM, disk, IP address, OS, uptime, tags, template status, backup status, and guest agent status.
- Filters: running, stopped, no backup, high CPU, high memory, no guest agent, node, tag, storage.

## Guest Agent Health

- Show guest agent installed/running state, last response time, IP detection, filesystem availability, and freeze/thaw support.
- Alerts: guest agent not responding, important VM missing guest agent, VM has no detected IP.

## Snapshot Monitoring

- Track snapshot count, age, size when available, old snapshots, and descriptions.
- Alerts: snapshot older than threshold, too many snapshots, snapshot storage pressure.

## Storage And Thin Pool Monitoring

- Support ZFS, LVM, LVM-thin, Directory, NFS, Ceph RBD, and PBS datastore.
- Track thin pool data and metadata, NFS availability, Ceph pool health, directory usage, ISO/template storage growth.
- Alerts: LVM-thin metadata high, NFS unavailable, expected storage disabled, storage usage high.

## Ceph Health

- Optional Ceph collector.
- Monitor Ceph health, OSD up/down, MON quorum, degraded/stuck PGs, pool usage, nearfull/full states, recovery activity.
- Alerts: Ceph warning/error, OSD down, MON quorum lost, PG degraded, pool near full.

## Alert Channels

- Support Slack, Discord, Telegram, SMTP email, generic webhook, Alertmanager, Grafana OnCall, Opsgenie, PagerDuty, Gotify, and ntfy.sh.

## Alert Rules

- Ship default alert rules.
- Allow user-defined/manual rules per VM/LXC, service, CPU, RAM, storage, network, and guest state.
- Default examples: node CPU high, node memory high, storage thresholds, VM stopped unexpectedly, backup failed, no backup in 48 hours, ZFS degraded, SMART failed, certificate expiry, cluster quorum lost, guest agent not responding.
- Allow config threshold overrides.

## Navigation

- Target navigation: Overview, Nodes, Guests, Storage, Backups, Tasks, Logs, Alerts, Security, Settings.

## Deployment Modes

- Mode A: single-node mode, installed on one Proxmox node and monitoring the cluster through the Proxmox API.
- Mode B: agent/hub mode, installed on every Proxmox node with one Sentinel server aggregating local data.
- Improve and document the existing cluster mode concepts.

## Privacy Policy

- No cloud dependency.
- No telemetry by default.
- No data leaves the node unless webhooks are configured.
- Credentials are stored locally.
- Recommend read-only Proxmox API tokens.
