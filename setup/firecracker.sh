# Install zerobox binaries and Firecracker.
# Sourced by setup.sh — expects ARCH, BIN_DIR, ZEROBOX_VERSION, FIRECRACKER_VERSION, SETUP_MODE.

GITHUB_REPO="afshinm/zerobox"

# Resolve the project root (where Cargo.toml lives)
_project_dir() {
    cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd
}

# Given the path to the zerobox binary, return the agent binary in the same dir
_agent_bin() {
    local dir
    dir="$(dirname "$1")"
    echo "${dir}/zerobox-agent"
}

install_zerobox() {
    local project_dir
    project_dir="$(_project_dir)"

    # Check release first, fall back to debug
    local local_bin="${project_dir}/target/release/zerobox"
    if [[ ! -x "$local_bin" ]] && [[ -x "${project_dir}/target/debug/zerobox" ]]; then
        local_bin="${project_dir}/target/debug/zerobox"
    fi

    local installed="${BIN_DIR}/zerobox"
    local need_install=false
    local need_restart=false

    # --- Dev mode: symlink to target/release ---
    if [[ "$SETUP_MODE" == "dev" ]]; then
        if [[ ! -x "$local_bin" ]]; then
            fatal "No local build found at ${local_bin}. Run: cargo build --release"
        fi

        if [[ -L "$installed" ]] && [[ "$(readlink "$installed")" == "$local_bin" ]]; then
            info "Dev symlink already in place"
        else
            rm -f "$installed"
            ln -s "$local_bin" "$installed"
            info "Symlinked ${installed} -> ${local_bin}"
        fi

        # Agent
        local local_agent
        local_agent="$(_agent_bin "$local_bin")"
        local installed_agent="${BIN_DIR}/zerobox-agent"
        if [[ -x "$local_agent" ]]; then
            rm -f "$installed_agent"
            ln -s "$local_agent" "$installed_agent"
            info "Symlinked ${installed_agent} -> ${local_agent}"
        fi

        info "Dev mode: 'cargo build --release && sudo systemctl restart zerobox' to update"
        return 0
    fi

    # --- Reinstall mode: always install ---
    if [[ "$SETUP_MODE" == "reinstall" ]]; then
        need_install=true
    # --- Default mode: install if missing or newer ---
    elif [[ ! -f "$installed" ]]; then
        need_install=true
    elif [[ -x "$local_bin" ]] && is_newer "$local_bin" "$installed"; then
        if confirm "A newer zerobox build was found. Upgrade?"; then
            need_install=true
            need_restart=true
        else
            info "Keeping existing zerobox"
            return 0
        fi
    else
        info "zerobox is up to date"
        return 0
    fi

    if [[ "$need_install" != "true" ]]; then
        return 0
    fi

    # --- Install from local build, GitHub release, or source ---

    # 1. Local build
    if [[ -x "$local_bin" ]]; then
        info "Installing zerobox from local build..."
        rm -f "$installed"
        install -m 0755 "$local_bin" "$installed"
        local local_agent
        local_agent="$(_agent_bin "$local_bin")"
        if [[ -x "$local_agent" ]]; then
            rm -f "${BIN_DIR}/zerobox-agent"
            install -m 0755 "$local_agent" "${BIN_DIR}/zerobox-agent"
        fi
        info "Installed ${installed}"
        if [[ "$need_restart" == "true" ]] && command_exists systemctl; then
            info "Restarting zerobox service..."
            systemctl restart zerobox.service 2>/dev/null || true
        fi
        return 0
    fi

    # 2. GitHub release
    local target
    case "${ARCH}" in
        x86_64)  target="x86_64-unknown-linux-gnu" ;;
        aarch64) target="aarch64-unknown-linux-gnu" ;;
    esac

    local tag="v${ZEROBOX_VERSION}"
    local archive="zerobox-daemon-${target}.tar.xz"
    local url="https://github.com/${GITHUB_REPO}/releases/download/${tag}/${archive}"

    info "Downloading zerobox ${tag} (${ARCH})..."
    local tmp
    tmp=$(mktemp -d)

    if curl -fSL -o "${tmp}/${archive}" "$url" 2>/dev/null; then
        tar -xJf "${tmp}/${archive}" -C "$tmp"
        local extract_dir="${tmp}/zerobox-daemon-${target}"
        if [[ -d "$extract_dir" ]]; then
            install -m 0755 "${extract_dir}/zerobox" "$installed"
            [[ -f "${extract_dir}/zerobox-agent" ]] && \
                install -m 0755 "${extract_dir}/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        elif [[ -f "${tmp}/zerobox" ]]; then
            install -m 0755 "${tmp}/zerobox" "$installed"
            [[ -f "${tmp}/zerobox-agent" ]] && \
                install -m 0755 "${tmp}/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        fi
        rm -rf "$tmp"
        info "Installed ${installed}"
        return 0
    fi
    rm -rf "$tmp"

    # 3. Build from source
    if command_exists cargo && [[ -f "${project_dir}/Cargo.toml" ]]; then
        info "Building zerobox from source..."
        (cd "$project_dir" && cargo build --release 2>&1 | tail -1)
        install -m 0755 "${project_dir}/target/release/zerobox" "$installed"
        install -m 0755 "${project_dir}/target/release/zerobox-agent" "${BIN_DIR}/zerobox-agent"
        info "Installed ${installed}"
        return 0
    fi

    fatal "Could not install zerobox. No local build, no release for ${tag}, and cargo not available."
}

install_firecracker() {
    if [[ "${ZEROBOX_SKIP_FC:-0}" == "1" ]]; then
        info "Skipping Firecracker (ZEROBOX_SKIP_FC=1)"
        return 0
    fi

    if [[ -x "${BIN_DIR}/firecracker" ]] && [[ "$SETUP_MODE" != "reinstall" ]]; then
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
