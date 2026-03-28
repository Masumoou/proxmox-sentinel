#!/bin/bash
# deploy.sh — Build and install proxmox-sentinel on a Proxmox node
# Run from the project root on your build machine, then copy binary to the node.

set -euo pipefail

BINARY="proxmox-sentinel"
REMOTE="${1:-root@your-proxmox-node}"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/proxmox-sentinel"

echo "=== Building release binary ==="
cargo build --release
strip target/release/$BINARY

echo "=== Copying to $REMOTE ==="
scp target/release/$BINARY "${REMOTE}:${INSTALL_DIR}/"
scp config.toml.example "${REMOTE}:${CONFIG_DIR}/config.toml.example"

echo "=== Installing systemd unit on $REMOTE ==="
ssh "$REMOTE" bash <<'REMOTE_SCRIPT'
set -e

# Create config dir if needed
mkdir -p /etc/proxmox-sentinel

# Install systemd unit
cat > /etc/systemd/system/proxmox-sentinel.service <<'UNIT'
[Unit]
Description=Proxmox Sentinel — agentless monitoring agent
After=network-online.target pve-cluster.service
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/proxmox-sentinel --config /etc/proxmox-sentinel/config.toml
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=proxmox-sentinel

# Security hardening — runs as root only because it needs:
#   - read /sys/fs/cgroup/lxc/*
#   - nsenter/pct exec into LXC namespaces
#   - read /var/lib/lxc/*/rootfs/
# If you don't need nsenter you can drop to a less privileged user.
User=root

# Resource limits on the sentinel itself
MemoryMax=64M
CPUQuota=5%

Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
systemctl enable proxmox-sentinel
echo "Done. Edit /etc/proxmox-sentinel/config.toml then:"
echo "  systemctl start proxmox-sentinel"
echo "  journalctl -fu proxmox-sentinel"
REMOTE_SCRIPT

echo ""
echo "=== Next steps ==="
echo "1. Edit config:  ssh $REMOTE vi /etc/proxmox-sentinel/config.toml"
echo "2. Create API token in Proxmox UI:"
echo "   Datacenter → Permissions → API Tokens → Add"
echo "   User: root@pam, Token ID: monitoring"
echo "   Required roles: PVEAuditor (read-only is enough)"
echo "3. Start:  ssh $REMOTE systemctl start proxmox-sentinel"
echo "4. Check:  ssh $REMOTE curl -s http://localhost:9101/metrics | head -40"
echo ""
echo "Prometheus scrape config:"
echo "  - job_name: proxmox-sentinel"
echo "    static_configs:"
echo "      - targets: ['your-proxmox-node:9101']"
