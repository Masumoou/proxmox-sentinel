# Security Policy

## Supported Versions

Security fixes target the latest released version.

## Reporting a Vulnerability

Please open a private GitHub security advisory or contact the maintainers before public disclosure.

## Deployment Guidance

Do not expose Sentinel directly to the internet.

Recommended access patterns:

- VPN
- WireGuard
- Tailscale
- trusted internal network
- TLS reverse proxy with authentication

Sentinel stores configuration locally. API tokens and webhook secrets are plain-text in `config.toml`, so keep `/etc/proxmox-sentinel/config.toml` readable only by root.

## Minimum Proxmox API Permissions

Use a read-only API token where possible. Avoid full Administrator tokens.

Recommended minimum privileges:

```text
Sys.Audit
VM.Audit
Datastore.Audit
Pool.Audit
SDN.Audit      # only if SDN visibility is used
```

LXC cgroup and rootfs log collection require the Sentinel process to run as root on the Proxmox host.
