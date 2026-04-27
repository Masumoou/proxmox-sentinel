# Security Recommendations

- Use a read-only Proxmox API token.
- Keep `/etc/proxmox-sentinel/config.toml` readable only by root.
- Do not expose Sentinel directly to the internet.
- Prefer VPN, WireGuard, Tailscale, or a TLS reverse proxy with authentication.
- Treat security checks as posture findings, not automatic remediation.

Minimum Proxmox privileges:

```text
Sys.Audit
VM.Audit
VM.GuestAgent.Audit          # guest-agent ping, OS, IP, filesystem reads
VM.GuestAgent.Unrestricted   # required for guest exec service/process discovery
Datastore.Audit
Pool.Audit
SDN.Audit      # only when SDN visibility is needed
```

If you do not want Sentinel to run guest exec inside VMs, omit `VM.GuestAgent.Unrestricted`; OS, IP, and filesystem visibility can still work with `VM.GuestAgent.Audit`.
