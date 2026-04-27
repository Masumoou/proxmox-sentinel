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

Sentinel uses native guest-agent endpoints for OS, IP, and filesystem data, then uses guest exec only for service and process discovery.

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
