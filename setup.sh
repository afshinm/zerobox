#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# zerobox setup
#
# Installs zerobox and its dependencies. On Linux, configures systemd.
# On macOS, sets up a Lima VM and runs setup inside it.
#
# Binary installation is handled by cargo-dist (generated installer).
# Systemd + deb packaging is handled by cargo-deb.
# This script handles the domain-specific parts: Firecracker, kernel, Lima.
#
# Usage:
#   sudo ./setup.sh            # interactive
#   ZEROBOX_YES=1 sudo ./setup.sh   # non-interactive
# ============================================================================

SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/setup" && pwd)"

source "${SETUP_DIR}/common.sh"
source "${SETUP_DIR}/detect.sh"
source "${SETUP_DIR}/firecracker.sh"
source "${SETUP_DIR}/kernel.sh"
source "${SETUP_DIR}/service.sh"
source "${SETUP_DIR}/lima.sh"

# --- Banner ----------------------------------------------------------------

print_banner() {
    cat << 'BANNER'

    ╺━┓┏━╸┏━┓┏━┓┏┓ ┏━┓╻ ╻
    ┏━┛┣╸ ┣┳┛┃ ┃┣┻┓┃ ┃┏╋┛
    ┗━╸┗━╸╹┗╸┗━┛┗━┛┗━┛╹ ╹

    Firecracker sandbox supervisor for AI agents

BANNER
    printf "    Version:  %s\n" "$ZEROBOX_VERSION"
    printf "    Platform: %s/%s\n\n" "$(uname -s | tr '[:upper:]' '[:lower:]')" "$(uname -m)"
}

# --- Confirmation ----------------------------------------------------------

confirm_linux_install() {
    cat << MSG
  This will install:

    - zerobox daemon + CLI   -> ${BIN_DIR}/zerobox
    - zerobox guest agent    -> ${BIN_DIR}/zerobox-agent
    - Firecracker ${FIRECRACKER_VERSION}         -> ${BIN_DIR}/firecracker
    - Linux kernel for VMs   -> ${KERNEL_DIR}/
    - Default config         -> ${CONFIG_DIR}/config.yaml
    - Data directory         -> ${DATA_DIR}/
    - systemd service        -> zerobox.service

MSG

    if ! confirm "Proceed with installation?"; then
        printf "\n  Aborted.\n\n"
        exit 0
    fi
    printf "\n"
}

# --- Summary ---------------------------------------------------------------

print_summary() {
    cat << SUMMARY

  ================================================
    zerobox ${ZEROBOX_VERSION} installed
  ================================================

  Binary:   ${BIN_DIR}/zerobox
  Config:   ${CONFIG_DIR}/config.yaml
  Data:     ${DATA_DIR}/
  Service:  zerobox.service

  Get started:
    zerobox start --image base
    zerobox exec <id> -- uname -a
    zerobox connect <id>
    zerobox destroy <id>

  Manage the service:
    sudo systemctl status zerobox
    sudo journalctl -u zerobox -f

SUMMARY
}

# --- Main ------------------------------------------------------------------

main() {
    print_banner
    detect_platform

    if [[ "$OS" == "darwin" ]]; then
        setup_macos
        return
    fi

    check_root
    confirm_linux_install
    check_prerequisites
    create_user
    create_directories
    install_zerobox
    install_firecracker
    install_kernel
    install_config
    install_systemd_service
    start_service
    print_summary
}

main "$@"
