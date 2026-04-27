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

Sentinel uses native guest-agent endpoints for OS, IP, and filesystem data, then uses guest exec only for service and process discovery. Guest exec is sent with Proxmox's JSON array payload:

```json
{"command":["/bin/sh","-lc","systemctl list-units --type=service --all --no-pager --no-legend --plain"]}
```

Plain `systemctl` output is parsed into service name, load state, active state, sub-state, description, `running`, and `failed`. A service is treated as running only when the active state is `active` and the sub-state is `running`.

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
