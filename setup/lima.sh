# macOS setup via Lima VM.
# Sourced by setup.sh — runs when OS=darwin.

setup_macos() {
    cat << 'MSG'
  zerobox requires Linux with KVM to run Firecracker microVMs.
  On macOS, this script will set up a Lima VM with nested
  virtualization and install zerobox inside it.

  Prerequisites: brew install lima

MSG

    if ! command_exists limactl; then
        fatal "Lima is not installed. Install it with: brew install lima"
    fi

    if ! confirm "Create and configure a Lima VM for zerobox?"; then
        printf "\n  Aborted.\n\n"
        exit 0
    fi

    printf "\n"
    local vm_name="zerobox-dev"

    if ! limactl list -q 2>/dev/null | grep -q "^${vm_name}$"; then
        info "Creating Lima VM '${vm_name}' (this takes a few minutes)..."
        limactl create --vm-type vz \
            --set '.nestedVirtualization=true' \
            --set '.cpus=6' \
            --set '.memory="8GiB"' \
            --set '.mountType="virtiofs"' \
            --name "$vm_name" \
            template://ubuntu-24.04
    fi

    if ! limactl list --json 2>/dev/null | grep -q '"status":"Running"'; then
        info "Starting Lima VM..."
        limactl start "$vm_name"
    else
        info "Lima VM '${vm_name}' is already running"
    fi

    if limactl shell "$vm_name" -- test -e /dev/kvm 2>/dev/null; then
        info "KVM is available inside the VM"
    else
        fatal "/dev/kvm not available. Your Mac may not support nested virtualization."
    fi

    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

    info "Running setup inside the Lima VM..."
    printf "\n"

    limactl shell "$vm_name" -- sudo env \
        ZEROBOX_VERSION="$ZEROBOX_VERSION" \
        FIRECRACKER_VERSION="$FIRECRACKER_VERSION" \
        ZEROBOX_YES=1 \
        bash "${script_dir}/setup.sh"

    cat << MSG

  Lima VM is ready. To use zerobox:

    limactl shell ${vm_name}
    zerobox list
    zerobox start --image base

MSG
    exit 0
}
