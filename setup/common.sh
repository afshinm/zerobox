# Common variables and helpers — sourced by all setup scripts
# Do not execute directly.

ZEROBOX_VERSION="${ZEROBOX_VERSION:-0.1.0}"
FIRECRACKER_VERSION="${FIRECRACKER_VERSION:-v1.10.1}"
ZEROBOX_USER="${ZEROBOX_USER:-zerobox}"
ZEROBOX_YES="${ZEROBOX_YES:-0}"

# Modes: default, dev, reinstall (set by setup.sh arg parsing)
SETUP_MODE="${SETUP_MODE:-default}"

BIN_DIR="/usr/local/bin"
CONFIG_DIR="/etc/zerobox"
DATA_DIR="/var/lib/zerobox"
KERNEL_DIR="${DATA_DIR}/kernels"
IMAGES_DIR="${DATA_DIR}/images"
SANDBOXES_DIR="${DATA_DIR}/sandboxes"
SNAPSHOTS_DIR="${DATA_DIR}/snapshots"

info()  { printf "  \033[32m>\033[0m %s\n" "$1"; }
warn()  { printf "  \033[33m!\033[0m %s\n" "$1"; }
error() { printf "  \033[31mx\033[0m %s\n" "$1" >&2; }
fatal() { error "$1"; exit 1; }

command_exists() { command -v "$1" >/dev/null 2>&1; }

confirm() {
    if [[ "$ZEROBOX_YES" == "1" ]]; then
        return 0
    fi
    printf "\n  %s [y/N] " "$1"
    read -r answer
    case "$answer" in
        [yY]|[yY][eE][sS]) return 0 ;;
        *) return 1 ;;
    esac
}

# Check if file A is newer than file B
is_newer() {
    local src="$1" dst="$2"
    if [[ ! -f "$dst" ]]; then
        return 0  # dst doesn't exist — "newer"
    fi
    if [[ ! -f "$src" ]]; then
        return 1  # src doesn't exist — not newer
    fi
    [[ "$src" -nt "$dst" ]]
}
