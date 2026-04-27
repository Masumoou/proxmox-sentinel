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

TMP_DIR="$(mktemp -d /tmp/proxmox-sentinel-install.XXXXXX)"
trap 'rm -rf "${TMP_DIR}"' EXIT

asset_url() {
    local asset_name="$1"
    echo "$RELEASE_JSON" \
        | grep -o "\"browser_download_url\"[[:space:]]*:[[:space:]]*\"[^\"]*${asset_name}\"" \
        | head -1 \
        | sed -E 's/.*"([^"]+)"/\1/' \
        || true
}

download_asset() {
    local url="$1"
    local dest="$2"
    curl -fSL --retry 3 --retry-delay 2 -o "$dest" "$url"
}

verify_checksum() {
    local file="$1"
    local asset_name="$2"

    if [[ ! -s "${TMP_DIR}/checksums.txt" ]]; then
        warn "checksums.txt unavailable; skipping checksum verification for ${asset_name}"
        return 0
    fi

    local expected
    expected="$(awk -v name="$asset_name" '$2 == name { print $1; exit }' "${TMP_DIR}/checksums.txt" || true)"
    if [[ -z "$expected" ]]; then
        warn "No checksum entry for ${asset_name}; skipping verification"
        return 0
    fi

    echo "${expected}  ${file}" | sha256sum -c -
}

run_first_time_init() {
    if [[ "${HAD_CONFIG}" == true ]]; then
        warn "Config already exists: ${CONFIG_DIR}/config.toml (preserved)"
        CONFIG_READY=true
        return 0
    fi

    if [[ -r /dev/tty && -w /dev/tty ]]; then
        info "Running interactive first-time setup..."
        "${INSTALL_DIR}/${BINARY_NAME}" init --force </dev/tty
        CONFIG_READY=true
        ok "Config written: ${CONFIG_DIR}/config.toml"
    else
        CONFIG_READY=false
        warn "Non-interactive install detected; config was not initialized."
        echo "  Run this next:"
        echo "  sudo ${INSTALL_DIR}/${BINARY_NAME} init"
    fi
}

# ── Download latest release ───────────────────────────────────────────────────

info "Fetching latest release from GitHub..."

HAD_CONFIG=false
[[ -f "${CONFIG_DIR}/config.toml" ]] && HAD_CONFIG=true
CONFIG_READY=false

RELEASE_JSON=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null || true)
DEB_ASSET="${BINARY_NAME}.deb"
BINARY_ASSET="${BINARY_NAME}-linux-amd64"
SERVICE_ASSET="${BINARY_NAME}.service"
CHECKSUM_ASSET="checksums.txt"

DEB_URL=$(asset_url "$DEB_ASSET")
BINARY_URL=$(asset_url "$BINARY_ASSET")
SERVICE_URL=$(asset_url "$SERVICE_ASSET")
CHECKSUM_URL=$(asset_url "$CHECKSUM_ASSET")

if [[ -n "$CHECKSUM_URL" ]]; then
    info "Downloading checksums.txt"
    download_asset "$CHECKSUM_URL" "${TMP_DIR}/checksums.txt"
fi

if [[ -n "$DEB_URL" ]] && command -v dpkg >/dev/null 2>&1; then
    info "Downloading Debian package: ${DEB_URL}"
    TMP_DEB="${TMP_DIR}/${DEB_ASSET}"
    download_asset "${DEB_URL}" "${TMP_DEB}"
    verify_checksum "${TMP_DEB}" "${DEB_ASSET}"
    dpkg -i "${TMP_DEB}" || DEBIAN_FRONTEND=noninteractive apt-get install -f -y
    ok "Debian package installed"
    run_first_time_init
    if [[ "${CONFIG_READY}" == true ]]; then
        systemctl enable --now proxmox-sentinel || true
    else
        warn "Service not started because first-time config is not initialized yet."
    fi
    echo -e "${CYAN}Installation complete!${NC}"
    exit 0
fi

if [[ -z "$BINARY_URL" ]]; then
    # Fallback: try GitHub Actions artifact (manual download needed)
    warn "No usable GitHub Release asset found for ${BINARY_ASSET}."
    echo ""
    echo "  A release must contain:"
    echo "    - ${BINARY_ASSET}"
    echo "    - ${DEB_ASSET}"
    echo "    - ${CHECKSUM_ASSET}"
    echo ""
    echo "  Push a version tag such as v0.3.0-beta, or download a CI artifact manually:"
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
    info "Downloading: ${BINARY_URL}"
    download_asset "${BINARY_URL}" "${INSTALL_DIR}/${BINARY_NAME}"
    verify_checksum "${INSTALL_DIR}/${BINARY_NAME}" "${BINARY_ASSET}"
fi

chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
ok "Binary installed: ${INSTALL_DIR}/${BINARY_NAME}"

# ── Create config directory ───────────────────────────────────────────────────

mkdir -p "${CONFIG_DIR}"
mkdir -p "${DATA_DIR}"

run_first_time_init

# ── Create systemd service ────────────────────────────────────────────────────

info "Creating systemd service..."

if [[ -n "${SERVICE_URL}" ]]; then
    download_asset "${SERVICE_URL}" "${TMP_DIR}/${SERVICE_ASSET}"
    verify_checksum "${TMP_DIR}/${SERVICE_ASSET}" "${SERVICE_ASSET}"
    cp "${TMP_DIR}/${SERVICE_ASSET}" /etc/systemd/system/proxmox-sentinel.service
else
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
fi

systemctl daemon-reload
ok "Systemd service created"

# ── Start service ─────────────────────────────────────────────────────────────

if [[ "${CONFIG_READY}" == true ]]; then
    info "Starting proxmox-sentinel..."
    systemctl enable --now proxmox-sentinel
else
    warn "Service not started because first-time config is not initialized yet."
    echo "  After init, run: sudo systemctl enable --now proxmox-sentinel"
fi

sleep 2

if [[ "${CONFIG_READY}" == true ]] && systemctl is-active --quiet proxmox-sentinel; then
    ok "proxmox-sentinel is running!"
    echo ""
    echo -e "  ${GREEN}Dashboard:${NC}  http://$(hostname -I | awk '{print $1}'):9101/"
    echo -e "  ${GREEN}Metrics:${NC}   http://$(hostname -I | awk '{print $1}'):9101/metrics"
    echo -e "  ${GREEN}Logs:${NC}      journalctl -fu proxmox-sentinel"
    echo -e "  ${GREEN}Config:${NC}    ${CONFIG_DIR}/config.toml"
    echo ""
elif [[ "${CONFIG_READY}" == true ]]; then
    warn "Service may not have started. Check: journalctl -fu proxmox-sentinel"
fi

echo -e "${CYAN}Installation complete!${NC}"
