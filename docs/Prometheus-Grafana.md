# Prometheus and Grafana

Prometheus endpoint:

```text
http://YOUR-PROXMOX-NODE:9101/metrics
```

Common metrics:

- `pve_node_cpu_usage_ratio`
- `pve_guest_cpu_usage_ratio`
- `pve_storage_used_bytes`
- `pve_platform_health_status`
- `pve_platform_health_value`

If dashboard auth is enabled, protect Prometheus scraping with the same credentials or put Sentinel behind a trusted reverse proxy.
