# Backup Policy

Sentinel checks real backup artifacts from Proxmox storage content APIs and local vzdump paths, then applies backup policy.

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

Tag rules:

```toml
[[backup_policy.tag_rules]]
tag = "critical"
warn_hours = 24
critical_hours = 36
required = true

[[backup_policy.tag_rules]]
tag = "nobackup"
required = false
```
