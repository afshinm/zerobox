# macOS setup via Lima VM.
# Sourced by setup.sh — runs when OS=darwin.
# Builds Linux binaries on the Mac via Docker, copies them into the VM.

_lima_build_linux_binaries() {
    local project_dir="$1"
    local arch="$2"
    local target

    case "$arch" in
        aarch64) target="aarch64-unknown-linux-musl" ;;
        x86_64)  target="x86_64-unknown-linux-musl" ;;
    esac

    local bin_path="${project_dir}/target/${target}/release"

    # Skip if already built and up to date
    if [[ -x "${bin_path}/zerobox" ]]; then
        local newest_src
        newest_src=$(find "${project_dir}/crates" -name '*.rs' -newer "${bin_path}/zerobox" 2>/dev/null | head -1)
        if [[ -z "$newest_src" ]]; then
            info "Linux binaries are up to date"
            return 0
        fi
    fi

    if ! command_exists docker; then
        fatal "Docker is required to build Linux binaries on macOS. Install Docker Desktop first."
    fi

    info "Building Linux binaries via Docker (target: ${target})..."
    docker run --rm \
        -v "${project_dir}":/src \
        -w /src \
        --platform "linux/${arch}" \
        rust:latest \
        bash -c "
            apt-get update -qq && apt-get install -y -qq musl-tools >/dev/null 2>&1
            rustup target add ${target} >/dev/null 2>&1
            cargo build --release --target ${target} -p zerobox-daemon -p zerobox-guest-agent
        " 2>&1 | tail -1

    if [[ ! -x "${bin_path}/zerobox" ]]; then
        fatal "Build failed — ${bin_path}/zerobox not found"
    fi

    info "Built: ${bin_path}/zerobox"
}

setup_macos() {
    local vm_name="zerobox-dev"

    if ! command_exists limactl; then
        fatal "Lima is not installed. Install it with: brew install lima"
    fi

    local vm_exists=false
    local vm_running=false
    if limactl list -q 2>/dev/null | grep -q "^${vm_name}$"; then
        vm_exists=true
        if limactl list --json 2>/dev/null | grep -q '"status":"Running"'; then
            vm_running=true
        fi
    fi

    local project_dir
    project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

    # Determine VM architecture
    local vm_arch
    case "$(uname -m)" in
        arm64|aarch64) vm_arch="aarch64" ;;
        x86_64|amd64)  vm_arch="x86_64" ;;
        *)             fatal "Unsupported architecture: $(uname -m)" ;;
    esac

    # --- Dev mode ---
    if [[ "$SETUP_MODE" == "dev" ]]; then
        if [[ "$vm_running" != "true" ]]; then
            if [[ "$vm_exists" == "true" ]]; then
                info "Starting Lima VM..."
                limactl start "$vm_name"
            else
                fatal "No Lima VM found. Run './setup.sh' first to create one."
            fi
        fi

        _lima_build_linux_binaries "$project_dir" "$vm_arch"

        local target_dir="target/${vm_arch}-unknown-linux-musl/release"

        info "Copying binaries to VM..."
        limactl shell "$vm_name" -- sudo bash -c "
            mkdir -p /opt/zerobox/target/release
            chown -R \$(logname):\$(logname) /opt/zerobox
        "
        limactl copy "${project_dir}/${target_dir}/zerobox" "${vm_name}:/opt/zerobox/target/release/zerobox"
        limactl copy "${project_dir}/${target_dir}/zerobox-agent" "${vm_name}:/opt/zerobox/target/release/zerobox-agent"

        # Sync setup scripts
        limactl shell "$vm_name" -- bash -c "
            rsync -a --delete \
                --exclude target/ --exclude node_modules/ --exclude .git/ --exclude data/ \
                '${project_dir}/' /opt/zerobox/
            cp /opt/zerobox/target/release/zerobox /opt/zerobox/target/release/zerobox 2>/dev/null || true
        "

        info "Running setup --dev inside VM..."
        limactl shell "$vm_name" -- sudo env \
            ZEROBOX_VERSION="$ZEROBOX_VERSION" \
            ZEROBOX_YES=1 \
            SETUP_MODE=dev \
            bash /opt/zerobox/setup.sh --dev

        cat << MSG

  Dev mode ready. After code changes:

    ./setup.sh --dev    # rebuilds Linux binaries via Docker, copies to VM
    limactl shell ${vm_name}
    sudo systemctl restart zerobox
    zerobox list

MSG
        exit 0
    fi

    # --- Default / reinstall ---
    cat << 'MSG'
  zerobox requires Linux with KVM to run Firecracker microVMs.
  On macOS, this script will:

    1. Build Linux binaries via Docker
    2. Create/start a Lima VM with nested virtualization
    3. Install zerobox + Firecracker + kernel inside the VM

MSG

    if ! confirm "Proceed?"; then
        printf "\n  Aborted.\n\n"
        exit 0
    fi
    printf "\n"

    # Build Linux binaries on Mac via Docker
    _lima_build_linux_binaries "$project_dir" "$vm_arch"

    # Create/start Lima VM
    if [[ "$vm_exists" != "true" ]]; then
        info "Creating Lima VM '${vm_name}'..."
        info "This downloads Ubuntu and boots a VM. First time takes 3-5 minutes."
        info "You'll see Lima's boot progress below:"
        printf "\n"
        limactl create --vm-type vz \
            --set '.nestedVirtualization=true' \
            --set '.cpus=6' \
            --set '.memory="8GiB"' \
            --set '.mountType="virtiofs"' \
            --name "$vm_name" \
            template://ubuntu-24.04
        printf "\n"
        info "VM created"
    fi

    if [[ "$vm_running" != "true" ]]; then
        info "Starting Lima VM (may take 1-2 minutes on first boot)..."
        limactl start "$vm_name"
    else
        info "Lima VM '${vm_name}' is already running"
    fi

    if limactl shell "$vm_name" -- test -e /dev/kvm 2>/dev/null; then
        info "KVM is available inside the VM"
    else
        fatal "/dev/kvm not available. Your Mac may not support nested virtualization."
    fi

    # Copy binaries and setup scripts into VM
    local target_dir="target/${vm_arch}-unknown-linux-musl/release"

    info "Copying binaries to VM..."
    limactl shell "$vm_name" -- sudo bash -c "
        mkdir -p /opt/zerobox/target/release
        chown -R \$(logname):\$(logname) /opt/zerobox
    "
    limactl copy "${project_dir}/${target_dir}/zerobox" "${vm_name}:/opt/zerobox/target/release/zerobox"
    limactl copy "${project_dir}/${target_dir}/zerobox-agent" "${vm_name}:/opt/zerobox/target/release/zerobox-agent"
    limactl shell "$vm_name" -- chmod +x /opt/zerobox/target/release/zerobox /opt/zerobox/target/release/zerobox-agent

    info "Syncing setup scripts to VM..."
    limactl shell "$vm_name" -- bash -c "
        rsync -a --delete \
            --exclude target/ --exclude node_modules/ --exclude .git/ --exclude data/ \
            '${project_dir}/' /opt/zerobox/
    "
    # Put the binaries back (rsync excluded target/)
    limactl copy "${project_dir}/${target_dir}/zerobox" "${vm_name}:/opt/zerobox/target/release/zerobox"
    limactl copy "${project_dir}/${target_dir}/zerobox-agent" "${vm_name}:/opt/zerobox/target/release/zerobox-agent"
    limactl shell "$vm_name" -- chmod +x /opt/zerobox/target/release/zerobox /opt/zerobox/target/release/zerobox-agent

    info "Running setup inside the VM..."
    printf "\n"

    local mode_flag=""
    [[ "$SETUP_MODE" == "reinstall" ]] && mode_flag="--reinstall"

    limactl shell "$vm_name" -- sudo env \
        ZEROBOX_VERSION="$ZEROBOX_VERSION" \
        FIRECRACKER_VERSION="$FIRECRACKER_VERSION" \
        ZEROBOX_YES=1 \
        bash /opt/zerobox/setup.sh $mode_flag

    cat << MSG

  Lima VM is ready. To use zerobox:

    limactl shell ${vm_name}
    zerobox list
    zerobox start --image base

MSG
    exit 0
}
