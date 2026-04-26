# Contributing

Thanks for helping improve Proxmox Sentinel.

## Development Checks

Run these before opening a pull request:

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all
cd frontend && npm ci && npm run build
```

## Scope

Keep core changes focused on Proxmox health monitoring:

- guests, storage, backups, tasks, snapshots
- ZFS, LVM-thin, Ceph, guest agents
- alerts, dashboard, Prometheus, packaging

Optional integrations such as HAProxy, Postgres, Redis, app logs, and app metrics should stay isolated.

## Pull Requests

Use small PRs when possible. Include:

- what changed
- why it changed
- how it was tested
- any compatibility or migration notes
