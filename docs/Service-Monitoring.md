# Service Monitoring

LXC services are collected from the Proxmox host with `pct exec`.

QEMU VM services require one of:

- QEMU Guest Agent installed and responsive
- SSH fallback configured in `[ssh]`

For QEMU Guest Agent service discovery, the Proxmox token needs:

```text
VM.GuestAgent.Audit
VM.GuestAgent.Unrestricted
```

Sentinel uses native guest-agent endpoints for OS, IP, and filesystem data, then uses guest exec only for service and process discovery. Guest exec is sent with Proxmox's JSON array payload.

Sentinel does not collect a hardcoded service allow-list. It collects every service that `systemctl` reports, then uses classification only for sorting and dashboard preview.

```json
{"command":["/bin/sh","-lc","(systemctl list-units --type=service --all --no-pager --no-legend --plain; systemctl list-units --type=service --state=running --no-pager --no-legend --plain; systemctl --failed --type=service --no-pager --no-legend --plain)"]}
```

Plain `systemctl` output is parsed into service name, load state, active state, sub-state, description, `running`, `failed`, and classification. A service is treated as running only when the active state is `active` and the sub-state is `running`.

Service states:

```text
running    active=active and sub=running
exited     active=active and sub=exited
inactive   active=inactive
failed     active=failed or sub=failed
not-found  load=not-found
```

Classification is display-only:

```text
web         apache2, nginx, caddy, traefik
php         php*-fpm, php-fpm
database    mysql, mariadb, postgresql, redis, mongodb
container   docker, containerd, podman
proxy/lb    haproxy, keepalived
monitoring  prometheus, grafana, node_exporter, zabbix-agent
system      systemd-*, dbus, cron, rsyslog, qemu-guest-agent
other       everything else
```

The compact dashboard preview shows failed services first, then application services like `apache2`, `php8.3-fpm`, and `ssh`, then lower-priority system services. The full guest service table still exposes every discovered service.

When `ss` is available inside the guest, Sentinel also runs `ss -lntup` and maps common process names to listening ports where possible, such as `apache2 -> 80,443`, `php-fpm8.3 -> 9000`, and `sshd -> 22`.

Service rules can target a VM/LXC by `vmid` and can evaluate:

```text
running, failed, inactive, dead, activating, unknown, missing
```

Configured VM service checks support both `vmid` and IP fallback:

```toml
[[services.vm]]
vmid = 101
ip = "192.168.1.50"
checks = ["nginx", "postgresql"]
```

Custom alert rules can target any collected service, not only predefined important services:

```toml
[[alert_rules]]
target = "service"
vmid = 104
service = "php8.3-fpm"
condition = "down"
duration_secs = 60
severity = "critical"
```
