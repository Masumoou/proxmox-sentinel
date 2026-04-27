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

- VM services are missing: install QEMU Guest Agent, grant `VM.GuestAgent.Audit` plus `VM.GuestAgent.Unrestricted`, or configure SSH fallback.
- Doctor reports `501` or `405` for guest-agent APIs: the client/server method map is mismatched. Proxmox requires `POST` for `/agent/ping` and `/agent/exec`, and `GET` for `/agent/get-osinfo`, `/agent/network-get-interfaces`, `/agent/get-fsinfo`, and `/agent/exec-status`.
- VM shows `agent=true` but `services=0`: native guest-agent reads are working, but guest exec failed or returned no service output. Check logs for `guest-agent exec_error`, and confirm the token has `VM.GuestAgent.Unrestricted`.
- Stopped templates trigger alerts: set `[alerts].ignore_template_guests = true`. This is the default for new configs.
- Doctor reports permission denied for guest-agent APIs: add `VM.GuestAgent.Audit`; add `VM.GuestAgent.Unrestricted` when service/process discovery through guest exec is enabled.
- `/metrics` returns unauthorized: configure Prometheus with Basic Auth or disable dashboard auth on trusted localhost-only deployments.
- Backup alerts are noisy: use `exclude_vmids`, `exclude_tags`, and tag rules.
- ZFS/Ceph data is missing: the host may not use that feature, or the command may not be installed.
