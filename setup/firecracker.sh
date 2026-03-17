# Install zerobox binaries and Firecracker.
# Sourced by setup.sh — expects ARCH, BIN_DIR, ZEROBOX_VERSION, FIRECRACKER_VERSION.

GITHUB_REPO="afshinm/zerobox"

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

    # 1. Try local build (dev workflow — running setup.sh from the repo)
    if [[ -x "${script_dir}/target/release/zerobox" ]]; then
        info "Installing zerobox from local build..."
        install -m 0755 "${script_dir}/target/release/zerobox" "${BIN_DIR}/zerobox"
        if [[ -x "${script_dir}/target/release/zerobox-agent" ]]; then
            install -m 0755 "${script_dir}/target/release/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        fi
        info "Installed ${BIN_DIR}/zerobox"
        return 0
    fi

    # 2. Try downloading pre-built binary from GitHub Releases (production workflow)
    local target
    case "${ARCH}" in
        x86_64)  target="x86_64-unknown-linux-gnu" ;;
        aarch64) target="aarch64-unknown-linux-gnu" ;;
    esac

    local release_url="https://github.com/${GITHUB_REPO}/releases"
    local tag="v${ZEROBOX_VERSION}"
    local archive="zerobox-daemon-${target}.tar.xz"
    local url="${release_url}/download/${tag}/${archive}"

    info "Downloading zerobox ${tag} (${ARCH})..."
    local tmp
    tmp=$(mktemp -d)

    if curl -fSL -o "${tmp}/${archive}" "$url" 2>/dev/null; then
        tar -xJf "${tmp}/${archive}" -C "$tmp"
        # cargo-dist extracts to a directory with the archive name (minus extension)
        local extract_dir="${tmp}/zerobox-daemon-${target}"
        if [[ -d "$extract_dir" ]]; then
            install -m 0755 "${extract_dir}/zerobox" "${BIN_DIR}/zerobox"
            [[ -f "${extract_dir}/zerobox-agent" ]] && \
                install -m 0755 "${extract_dir}/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        else
            # Flat archive — binaries at top level
            [[ -f "${tmp}/zerobox" ]] && install -m 0755 "${tmp}/zerobox" "${BIN_DIR}/zerobox"
            [[ -f "${tmp}/zerobox-agent" ]] && install -m 0755 "${tmp}/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        fi
        rm -rf "$tmp"
        info "Installed ${BIN_DIR}/zerobox"
        return 0
    fi
    rm -rf "$tmp"

    # 3. Fall back to building from source
    if command_exists cargo && [[ -f "${script_dir}/Cargo.toml" ]]; then
        info "Building zerobox from source (this may take a minute)..."
        (cd "$script_dir" && cargo build --release 2>&1 | tail -1)
        install -m 0755 "${script_dir}/target/release/zerobox" "${BIN_DIR}/zerobox"
        install -m 0755 "${script_dir}/target/release/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        info "Installed ${BIN_DIR}/zerobox"
        return 0
    fi

    fatal "Could not install zerobox. No local build, no release binary for ${tag}, and cargo not available."
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
