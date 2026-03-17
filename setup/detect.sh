# Platform detection and prerequisite checks.
# Sourced by setup.sh — sets OS and ARCH globals.

detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$ARCH" in
        x86_64|amd64)  ARCH="x86_64" ;;
        aarch64|arm64) ARCH="aarch64" ;;
        *)             fatal "Unsupported architecture: $ARCH" ;;
    esac
}

check_root() {
    if [[ $EUID -ne 0 ]]; then
        fatal "This script must be run as root. Use: sudo $0"
    fi
}

check_prerequisites() {
    info "Checking prerequisites..."

    local missing=()

    if [[ ! -e /dev/kvm ]]; then
        error "/dev/kvm not found. KVM is required for Firecracker."
        printf "    Ensure virtualization is enabled in BIOS/firmware.\n"
        printf "    For cloud VMs, enable nested virtualization.\n"
        exit 1
    fi

    if ! command_exists ip; then missing+=("iproute2"); fi
    if ! command_exists iptables; then missing+=("iptables"); fi
    if ! command_exists curl; then missing+=("curl"); fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        info "Installing missing packages: ${missing[*]}"
        if command_exists apt-get; then
            apt-get update -qq
            apt-get install -y -qq "${missing[@]}"
        elif command_exists dnf; then
            dnf install -y -q "${missing[@]}"
        elif command_exists yum; then
            yum install -y -q "${missing[@]}"
        elif command_exists apk; then
            apk add --quiet "${missing[@]}"
        else
            fatal "Cannot install packages. Please install manually: ${missing[*]}"
        fi
    fi

    info "Prerequisites OK"
}

create_user() {
    if id "$ZEROBOX_USER" &>/dev/null; then
        info "User '${ZEROBOX_USER}' already exists"
    else
        info "Creating system user '${ZEROBOX_USER}'..."
        useradd -r -s /usr/sbin/nologin -d "$DATA_DIR" -m "$ZEROBOX_USER"
    fi

    if getent group kvm >/dev/null 2>&1; then
        usermod -a -G kvm "$ZEROBOX_USER"
    fi
}

create_directories() {
    info "Creating directories..."
    mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$KERNEL_DIR" "$IMAGES_DIR" "$SANDBOXES_DIR" "$SNAPSHOTS_DIR"
    chown -R "${ZEROBOX_USER}:${ZEROBOX_USER}" "$DATA_DIR"
    chmod 750 "$DATA_DIR"
}
