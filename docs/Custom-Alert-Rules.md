# Custom Alert Rules

Rules live in `config.toml` as `[[alert_rules]]`.

Supported targets:

```text
node, vm, lxc, guest, service, storage
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

Service conditions:

```text
down, not_running, failed, inactive, dead, missing, running
```
