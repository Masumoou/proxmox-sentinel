# Real-World Testing Checklist

Use this checklist before publishing a public release. The goal is to prove Sentinel behaves across common Proxmox environments without adding noisy false positives.

## Install And Upgrade

- Fresh install with `curl -fsSL https://raw.githubusercontent.com/Masumoou/proxmox-sentinel/main/install.sh | sudo bash`
- Fresh install with `proxmox-sentinel.deb`
- Binary fallback install using `proxmox-sentinel-linux-amd64`
- Upgrade over an existing `/etc/proxmox-sentinel/config.toml`
- Checksum verification succeeds when `checksums.txt` is available
- Installer prints a clear message if no GitHub Release asset is available
- `proxmox-sentinel init` creates config and systemd service
- `proxmox-sentinel doctor` reports a running Sentinel instance when port `9101` is already in use by Sentinel

## Proxmox Topologies

- Single-node Proxmox host
- Multi-node Proxmox cluster
- Cluster with quorum healthy
- Cluster with one node temporarily offline
- Host with no ZFS installed
- Host with no Ceph installed
- Host with no HA resources configured

## Storage

- Proxmox with ZFS `rpool`
- Proxmox with an extra ZFS pool
- ZFS pool `ONLINE`
- ZFS pool `DEGRADED` or a fixture that simulates degraded status
- ZFS scrub with zero errors
- ZFS scrub with non-zero errors
- Proxmox with LVM-thin
- LVM-thin data warning threshold
- LVM-thin metadata warning threshold
- Directory storage
- NFS storage active
- NFS storage unavailable
- Ceph installed and `HEALTH_OK`
- Ceph installed and `HEALTH_WARN`
- PBS or backup-capable storage visible through Proxmox storage content API

## Guests

- QEMU VM with QEMU Guest Agent enabled and responding
- QEMU VM without QEMU Guest Agent
- QEMU VM with SSH fallback only
- QEMU VM with no guest visibility
- Linux VM running systemd
- Fedora VM running systemd
- Debian/Ubuntu VM running systemd
- VM service list from `systemctl --output=json`
- VM service list from plain `systemctl`
- VM with failed service
- VM with stopped service
- VM IP detected through guest agent
- VM IP detected through host ARP/neighbor fallback
- VM OS/version detected from `/etc/os-release`
- Stopped VM
- Template VM

## LXC Containers

- LXC with systemd
- LXC with OpenRC
- LXC where `pct exec systemctl` works
- LXC where `rc-status --nocolor` is needed
- LXC with service states: running, inactive, failed, dead
- LXC cgroup v2 CPU and memory files readable
- LXC rootfs logs accessible under `/var/lib/lxc/<vmid>/rootfs/var/log`
- Stopped LXC
- Template LXC

## Backups

- Backup storage with real `vzdump-qemu-*` files
- Backup storage with real `vzdump-lxc-*` files
- Proxmox storage content API returns backup rows
- Local `/var/lib/vz/dump` scan finds backup files
- `/mnt/pve/*/dump` scan finds backup files
- Guest with fresh backup
- Guest with stale backup
- Guest with no backup
- Guest excluded by `exclude_vmids`
- Guest excluded by `nobackup` tag
- Guest using stricter `critical` tag window
- Stopped guests ignored when configured
- Templates ignored when configured

## Tasks And Alerts

- Failed backup task
- Failed migration task
- Failed snapshot task
- Long-running task
- Custom alert rule: VM CPU threshold
- Custom alert rule: guest stopped
- Custom alert rule: service down
- Custom alert rule: storage usage
- Alert deduplication prevents repeated pages during the silence window
- Generic webhook receives Alertmanager-compatible payload

## Dashboard And Metrics

- Overview shows overall health within one poll cycle
- Guests page keeps data while navigating between pages
- Storage page shows Proxmox storage plus platform storage health
- Backups page shows backup freshness
- Tasks page shows task history
- Alerts page shows recent alerts
- Security page labels posture findings without implying auto-fix
- `/health` responds locally
- `/metrics` exposes Prometheus text output when enabled
- `/ws` requires dashboard auth when auth is configured
