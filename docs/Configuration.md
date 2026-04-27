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
- `[alerts]`: global alert thresholds, webhook URL, and template alert handling
- `[platform]`: ZFS, LVM-thin, tasks, snapshots, backups, security posture
- `[backup_policy]`: backup requirements and tag rules
- `[[alert_rules]]`: custom alert rules
- `[services]`: service discovery behavior
- `[ssh]`: optional VM SSH fallback

See `config.toml.example` for a complete reference.

By default, built-in guest-down alerts ignore VM/LXC templates:

```toml
[alerts]
ignore_template_guests = true
```

Custom alert rules are still evaluated if you explicitly target a template VMID.
