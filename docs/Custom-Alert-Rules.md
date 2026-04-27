# Custom Alert Rules

Rules can live in `config.toml` as `[[alert_rules]]` or be created from the dashboard Alerts page. UI-created rules are stored in SQLite and survive Sentinel restarts.

Supported targets:

```text
node, vm, lxc, guest, service, storage, guest_disk
```

Supported operators:

```text
>, >=, <, <=, ==, =, !=
```

`duration_secs` means the rule fires only after the condition remains true for that many seconds.

Example:

```toml
[[alert_rules]]
name = "vm-101-nginx-down"
target = "service"
vmid = 101
service = "nginx"
condition = "down"
duration_secs = 60
severity = "critical"
```

Guest filesystem usage rule:

```toml
[[alert_rules]]
name = "vm-104-root-disk-high"
target = "guest_disk"
vmid = 104
mount = "/"
metric = "used_percent"
operator = ">"
threshold = 85
duration_secs = 300
severity = "warning"
```

Service conditions:

```text
down, not_running, failed, inactive, dead, missing, running
```
