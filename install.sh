#!/usr/bin/env bash
# ──────────────────────────────────────────────────────────────────────────────
# Proxmox Sentinel — One-Command Installer
# Usage: curl -sSL https://raw.githubusercontent.com/Masumoou/proxmox-sentinel/main/install.sh | bash
# ──────────────────────────────────────────────────────────────────────────────

set -euo pipefail

REPO="Masumoou/proxmox-sentinel"
BINARY_NAME="proxmox-sentinel"
INSTALL_DIR="/usr/local/bin"
CONFIG_DIR="/etc/proxmox-sentinel"
DATA_DIR="/var/lib/proxmox-sentinel"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
YELLOW='\033[1;33m'
NC='\033[0m'

info() { echo -e "${CYAN}[INFO]${NC} $*"; }
ok()   { echo -e "${GREEN}[OK]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }
err()  { echo -e "${RED}[ERROR]${NC} $*" >&2; exit 1; }

# ── Preflight checks ──────────────────────────────────────────────────────────

echo -e "${CYAN}"
echo "  ╔══════════════════════════════════════════════════════════╗"
echo "  ║           Proxmox Sentinel — Installer                  ║"
echo "  ║     Agentless Proxmox monitoring in one binary          ║"
echo "  ╚══════════════════════════════════════════════════════════╝"
echo -e "${NC}"

[[ $EUID -ne 0 ]] && err "This script must be run as root (sudo)"
command -v curl >/dev/null 2>&1 || err "curl is required but not installed"

# ── Download latest release ───────────────────────────────────────────────────

info "Fetching latest release from GitHub..."

HAD_CONFIG=false
[[ -f "${CONFIG_DIR}/config.toml" ]] && HAD_CONFIG=true

RELEASE_JSON=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null || true)
DEB_URL=$(echo "$RELEASE_JSON" | grep -o "https://.*${BINARY_NAME}\.deb[^\"]*" | head -1 || true)
RELEASE_URL=$(echo "$RELEASE_JSON" | grep -o "https://.*${BINARY_NAME}-linux-amd64[^\"]*" | head -1 || true)

if [[ -n "$DEB_URL" ]] && command -v dpkg >/dev/null 2>&1; then
    info "Downloading Debian package: ${DEB_URL}"
    TMP_DEB="$(mktemp /tmp/proxmox-sentinel.XXXXXX.deb)"
    curl -sSL -o "${TMP_DEB}" "${DEB_URL}"
    dpkg -i "${TMP_DEB}" || apt-get install -f -y
    ok "Debian package installed"
    if [[ "${HAD_CONFIG}" != true ]]; then
        "${INSTALL_DIR}/${BINARY_NAME}" init --force
    fi
    systemctl enable --now proxmox-sentinel || true
    echo -e "${CYAN}Installation complete!${NC}"
    exit 0
fi

if [[ -z "$RELEASE_URL" ]]; then
    # Fallback: try GitHub Actions artifact (manual download needed)
    warn "No GitHub Release found. Checking for CI build artifact..."
    echo ""
    echo "  The binary isn't published as a Release yet."
    echo "  Download it manually from GitHub Actions:"
    echo "  https://github.com/${REPO}/actions"
    echo ""
    echo "  Then run: install.sh /path/to/downloaded/proxmox-sentinel"
    echo ""

    # If user passed a local path as argument
    if [[ $# -ge 1 && -f "$1" ]]; then
        info "Using local binary: $1"
        cp "$1" "${INSTALL_DIR}/${BINARY_NAME}"
    else
        err "No binary available. Download from GitHub Actions first."
    fi
else
    info "Downloading: ${RELEASE_URL}"
    curl -sSL -o "${INSTALL_DIR}/${BINARY_NAME}" "${RELEASE_URL}"
fi

chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
ok "Binary installed: ${INSTALL_DIR}/${BINARY_NAME}"

# ── Create config directory ───────────────────────────────────────────────────

mkdir -p "${CONFIG_DIR}"
mkdir -p "${DATA_DIR}"

if [[ ! -f "${CONFIG_DIR}/config.toml" ]]; then
    info "Running interactive first-time setup..."
    "${INSTALL_DIR}/${BINARY_NAME}" init --force
    ok "Config written: ${CONFIG_DIR}/config.toml"
else
    warn "Config already exists: ${CONFIG_DIR}/config.toml (skipping)"
fi

# ── Create systemd service ────────────────────────────────────────────────────

info "Creating systemd service..."

cat > /etc/systemd/system/proxmox-sentinel.service << 'UNIT'
[Unit]
Description=Proxmox Sentinel — Agentless Monitoring
After=network-online.target
Wants=network-online.target
Documentation=https://github.com/Masumoou/proxmox-sentinel

[Service]
Type=simple
ExecStart=/usr/local/bin/proxmox-sentinel --config /etc/proxmox-sentinel/config.toml
Restart=always
RestartSec=5
StandardOutput=journal
StandardError=journal
SyslogIdentifier=proxmox-sentinel

# Security hardening
NoNewPrivileges=false
ProtectSystem=strict
ReadWritePaths=/var/lib/proxmox-sentinel
ReadOnlyPaths=/sys/fs/cgroup /var/lib/lxc /proc /etc/proxmox-sentinel

[Install]
WantedBy=multi-user.target
UNIT

systemctl daemon-reload
ok "Systemd service created"

# ── Start service ─────────────────────────────────────────────────────────────

info "Starting proxmox-sentinel..."
systemctl enable --now proxmox-sentinel

sleep 2

if systemctl is-active --quiet proxmox-sentinel; then
    ok "proxmox-sentinel is running!"
    echo ""
    echo -e "  ${GREEN}Dashboard:${NC}  http://$(hostname -I | awk '{print $1}'):9101/"
    echo -e "  ${GREEN}Metrics:${NC}   http://$(hostname -I | awk '{print $1}'):9101/metrics"
    echo -e "  ${GREEN}Logs:${NC}      journalctl -fu proxmox-sentinel"
    echo -e "  ${GREEN}Config:${NC}    ${CONFIG_DIR}/config.toml"
    echo ""
else
    warn "Service may not have started. Check: journalctl -fu proxmox-sentinel"
fi

echo -e "${CYAN}Installation complete!${NC}"
