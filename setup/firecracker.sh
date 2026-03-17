# Install Firecracker and the zerobox binaries.
# Sourced by setup.sh — expects ARCH, BIN_DIR, FIRECRACKER_VERSION.

install_zerobox() {
    if [[ -f "${BIN_DIR}/zerobox" ]]; then
        local current
        current=$("${BIN_DIR}/zerobox" --help 2>/dev/null | head -1 || echo "")
        if [[ -n "$current" ]]; then
            info "zerobox already installed at ${BIN_DIR}/zerobox"
            return 0
        fi
    fi

    local script_dir
    script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

    if [[ -x "${script_dir}/target/release/zerobox" ]]; then
        info "Installing zerobox from local build..."
        install -m 0755 "${script_dir}/target/release/zerobox" "${BIN_DIR}/zerobox"
        if [[ -x "${script_dir}/target/release/zerobox-agent" ]]; then
            install -m 0755 "${script_dir}/target/release/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        fi
    elif command_exists cargo; then
        info "Building zerobox from source (this may take a minute)..."
        if [[ -f "${script_dir}/Cargo.toml" ]]; then
            (cd "$script_dir" && cargo build --release 2>&1 | tail -1)
            install -m 0755 "${script_dir}/target/release/zerobox" "${BIN_DIR}/zerobox"
            install -m 0755 "${script_dir}/target/release/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        else
            fatal "Cargo.toml not found. Run setup.sh from the zerobox project directory."
        fi
    else
        fatal "No pre-built binaries and cargo not installed. Install Rust first:\n    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    fi

    info "Installed ${BIN_DIR}/zerobox"
}

install_firecracker() {
    if [[ "${ZEROBOX_SKIP_FC:-0}" == "1" ]]; then
        info "Skipping Firecracker (ZEROBOX_SKIP_FC=1)"
        return 0
    fi

    if [[ -x "${BIN_DIR}/firecracker" ]]; then
        info "Firecracker already installed"
        return 0
    fi

    info "Installing Firecracker ${FIRECRACKER_VERSION} (${ARCH})..."

    local tmp
    tmp=$(mktemp -d)
    local tarball="firecracker-${FIRECRACKER_VERSION}-${ARCH}.tgz"
    local url="https://github.com/firecracker-microvm/firecracker/releases/download/${FIRECRACKER_VERSION}/${tarball}"

    curl -fSL -o "${tmp}/${tarball}" "$url" || fatal "Failed to download Firecracker"
    tar -xzf "${tmp}/${tarball}" -C "$tmp"

    local release_dir="${tmp}/release-${FIRECRACKER_VERSION}-${ARCH}"
    install -m 0755 "${release_dir}/firecracker-${FIRECRACKER_VERSION}-${ARCH}" "${BIN_DIR}/firecracker"
    if [[ -f "${release_dir}/jailer-${FIRECRACKER_VERSION}-${ARCH}" ]]; then
        install -m 0755 "${release_dir}/jailer-${FIRECRACKER_VERSION}-${ARCH}" "${BIN_DIR}/jailer"
    fi

    rm -rf "$tmp"
    info "Installed ${BIN_DIR}/firecracker"
}
