# Build base rootfs image for Firecracker VMs.
# Sourced by setup.sh — expects IMAGES_DIR, BIN_DIR, ZEROBOX_USER.

build_rootfs() {
    local rootfs_file="${IMAGES_DIR}/base.ext4"

    if [[ -f "$rootfs_file" ]] && [[ "$SETUP_MODE" != "reinstall" ]]; then
        info "Base rootfs already exists"
        return 0
    fi

    # Need Docker to build the rootfs
    if ! command_exists docker; then
        info "Installing Docker..."
        if command_exists apt-get; then
            apt-get update -qq
            apt-get install -y -qq docker.io >/dev/null 2>&1
        elif command_exists dnf; then
            dnf install -y -q docker
        else
            warn "Docker not found and cannot auto-install. Rootfs build skipped."
            warn "Install Docker, then run: images/build-rootfs.sh base"
            return 0
        fi
    fi

    # Make sure Docker daemon is running
    if ! docker info >/dev/null 2>&1; then
        systemctl start docker 2>/dev/null || true
        sleep 2
        if ! docker info >/dev/null 2>&1; then
            warn "Docker is not running. Rootfs build skipped."
            return 0
        fi
    fi

    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
    local build_script="${script_dir}/images/build-rootfs.sh"

    if [[ ! -f "$build_script" ]]; then
        warn "build-rootfs.sh not found. Rootfs build skipped."
        return 0
    fi

    info "Building base rootfs image (this takes about a minute)..."

    # build-rootfs.sh outputs to <project>/data/images/.
    # We need the image at IMAGES_DIR (/var/lib/zerobox/images/).
    # Set PROJECT_DIR to make build-rootfs.sh output there, or build and copy.

    # Create a temp project structure so build-rootfs.sh outputs correctly
    local tmp_project
    tmp_project=$(mktemp -d)
    mkdir -p "${tmp_project}/data/images"
    mkdir -p "${tmp_project}/images"
    cp "${script_dir}"/images/Dockerfile.* "${tmp_project}/images/"
    cp "${build_script}" "${tmp_project}/images/build-rootfs.sh"

    # If the guest agent is installed, use it
    if [[ -x "${BIN_DIR}/zerobox-agent" ]]; then
        mkdir -p "${tmp_project}/target/release"
        cp "${BIN_DIR}/zerobox-agent" "${tmp_project}/target/release/zerobox-agent"
    fi

    (cd "${tmp_project}" && bash ./images/build-rootfs.sh base 2>&1) | tail -5

    if [[ -f "${tmp_project}/data/images/base.ext4" ]]; then
        mv "${tmp_project}/data/images/base.ext4" "$rootfs_file"
        chown "${ZEROBOX_USER}:${ZEROBOX_USER}" "$rootfs_file"
        info "Built rootfs: ${rootfs_file}"
    else
        warn "Rootfs build failed. You can build it manually later:"
        warn "  images/build-rootfs.sh base"
    fi

    rm -rf "$tmp_project"
}
