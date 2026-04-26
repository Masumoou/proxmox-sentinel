# Service Monitoring

LXC services are collected from the Proxmox host with `pct exec`.

QEMU VM services require one of:

- QEMU Guest Agent installed and responsive
- SSH fallback configured in `[ssh]`

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
