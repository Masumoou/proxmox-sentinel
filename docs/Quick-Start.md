# Quick Start

Install the latest release:

```bash
curl -fsSL https://raw.githubusercontent.com/Masumoou/proxmox-sentinel/main/install.sh | sudo bash
sudo proxmox-sentinel init
sudo systemctl enable --now proxmox-sentinel
sudo proxmox-sentinel doctor
```

Open the dashboard:

```text
http://YOUR-PROXMOX-NODE:9101
```

Use a read-only Proxmox API token. Do not expose Sentinel directly to the internet.
