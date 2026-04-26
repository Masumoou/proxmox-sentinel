# Configuration

Main config path:

```text
/etc/proxmox-sentinel/config.toml
```

Generate it interactively:

```bash
sudo proxmox-sentinel init
```

Important sections:

- `[proxmox]`: API URL and token
- `[metrics]`: dashboard, Prometheus, and auth
- `[platform]`: ZFS, LVM-thin, tasks, snapshots, backups, security posture
- `[backup_policy]`: backup requirements and tag rules
- `[[alert_rules]]`: custom alert rules
- `[services]`: service discovery behavior
- `[ssh]`: optional VM SSH fallback

See `config.toml.example` for a complete reference.
