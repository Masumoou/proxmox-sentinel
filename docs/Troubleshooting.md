# Troubleshooting

Run:

```bash
sudo proxmox-sentinel doctor
```

Check service logs:

```bash
journalctl -u proxmox-sentinel -n 100 --no-pager
```

Common issues:

- VM services are missing: install QEMU Guest Agent or configure SSH fallback.
- `/metrics` returns unauthorized: configure Prometheus with Basic Auth or disable dashboard auth on trusted localhost-only deployments.
- Backup alerts are noisy: use `exclude_vmids`, `exclude_tags`, and tag rules.
- ZFS/Ceph data is missing: the host may not use that feature, or the command may not be installed.
