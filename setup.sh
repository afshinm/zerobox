#!/usr/bin/env bash
set -euo pipefail

# ============================================================================
# zerobox setup
#
# Usage:
#   sudo ./setup.sh               # install or upgrade if newer
#   sudo ./setup.sh --dev         # symlink to local build (for development)
#   sudo ./setup.sh --reinstall   # force reinstall everything
#   ZEROBOX_YES=1 sudo ./setup.sh # non-interactive
# ============================================================================

SETUP_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/setup" && pwd)"

# Parse flags before sourcing (so SETUP_MODE is set)
SETUP_MODE="default"
for arg in "$@"; do
    case "$arg" in
        --dev)       SETUP_MODE="dev" ;;
        --reinstall) SETUP_MODE="reinstall" ;;
        --yes|-y)    export ZEROBOX_YES=1 ;;
        --help|-h)
            cat << 'USAGE'
Usage: sudo ./setup.sh [OPTIONS]

Options:
  (default)     Install or upgrade zerobox if a newer build exists
  --dev         Symlink binary to target/release/ for fast dev iteration
  --reinstall   Force reinstall everything (keeps config)
  --yes, -y     Skip confirmation prompts
  --help, -h    Show this help

Dev workflow:
  sudo ./setup.sh --dev           # first time: symlink + setup everything
  cargo build --release           # make changes, rebuild
  sudo systemctl restart zerobox  # picks up new binary instantly

USAGE
            exit 0
            ;;
    esac
done
export SETUP_MODE

source "${SETUP_DIR}/common.sh"
source "${SETUP_DIR}/detect.sh"
source "${SETUP_DIR}/firecracker.sh"
source "${SETUP_DIR}/kernel.sh"
source "${SETUP_DIR}/rootfs.sh"
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
    printf "    Platform: %s/%s\n" "$(uname -s | tr '[:upper:]' '[:lower:]')" "$(uname -m)"
    if [[ "$SETUP_MODE" != "default" ]]; then
        printf "    Mode:     %s\n" "$SETUP_MODE"
    fi
    printf "\n"
}

# --- Confirmation ----------------------------------------------------------

confirm_linux_install() {
    if [[ "$SETUP_MODE" == "reinstall" ]]; then
        cat << 'MSG'
  Reinstall mode: will force reinstall all components.
  Config file will NOT be overwritten.

MSG
        if ! confirm "Proceed with reinstall?"; then
            printf "\n  Aborted.\n\n"
            exit 0
        fi
        printf "\n"
        return
    fi

    if [[ "$SETUP_MODE" == "dev" ]]; then
        cat << MSG
  Dev mode: will symlink binaries to local build.
  After code changes, just run:

    cargo build --release
    sudo systemctl restart zerobox

MSG
        if ! confirm "Proceed with dev setup?"; then
            printf "\n  Aborted.\n\n"
            exit 0
        fi
        printf "\n"
        return
    fi

    # Default mode
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
    if [[ "$SETUP_MODE" == "dev" ]]; then
        cat << SUMMARY

  ================================================
    zerobox ${ZEROBOX_VERSION} (dev mode)
  ================================================

  Binary:   ${BIN_DIR}/zerobox -> target/release/zerobox
  Config:   ${CONFIG_DIR}/config.yaml
  Service:  zerobox.service

  Dev workflow:
    cargo build --release
    sudo systemctl restart zerobox

SUMMARY
    else
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
    fi
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
    build_rootfs
    install_config
    install_systemd_service
    start_service
    print_summary
}

main "$@"
