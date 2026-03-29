#!/bin/sh
# Install zerobox — cross-platform process sandbox.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/afshinm/zerobox/main/install.sh | sh
#
# Options (env vars):
#   ZEROBOX_INSTALL  — install directory (default: $HOME/.zerobox)
#   ZEROBOX_VERSION  — specific version to install (default: latest)

set -eu

main() {
    need_cmd uname
    need_cmd chmod
    need_cmd mkdir
    need_cmd tar

    local _target
    _target="$(detect_target)"

    local _version
    _version="${ZEROBOX_VERSION:-latest}"

    local _install_dir
    _install_dir="${ZEROBOX_INSTALL:-$HOME/.zerobox}"
    local _bin_dir="$_install_dir/bin"

    local _base_url="https://github.com/afshinm/zerobox/releases"
    local _url
    if [ "$_version" = "latest" ]; then
        _url="$_base_url/latest/download/zerobox-${_target}.tar.gz"
    else
        _url="$_base_url/download/v${_version}/zerobox-${_target}.tar.gz"
    fi

    local _tmp
    _tmp="$(mktemp -d)"
    trap "rm -rf '$_tmp'" EXIT

    log "Downloading zerobox for $_target"
    download "$_url" "$_tmp/zerobox.tar.gz"

    log "Extracting to $_bin_dir"
    mkdir -p "$_bin_dir"
    tar xzf "$_tmp/zerobox.tar.gz" -C "$_tmp"
    mv "$_tmp/zerobox" "$_bin_dir/zerobox"
    chmod +x "$_bin_dir/zerobox"

    # Verify the binary runs.
    if ! "$_bin_dir/zerobox" --version >/dev/null 2>&1; then
        err "Downloaded binary failed to execute. Try installing from npm instead: npm install -g zerobox"
    fi

    log "Installed zerobox to $_bin_dir/zerobox"
    echo ""

    # Check if already in PATH.
    case ":${PATH}:" in
        *":$_bin_dir:"*) ;;
        *)
            log "Add zerobox to your PATH:"
            echo ""
            case "$(basename "${SHELL:-}")" in
                fish)
                    echo "  set -Ux fish_user_paths $_bin_dir \$fish_user_paths"
                    ;;
                zsh)
                    echo "  echo 'export PATH=\"$_bin_dir:\$PATH\"' >> ~/.zshrc"
                    ;;
                *)
                    echo "  echo 'export PATH=\"$_bin_dir:\$PATH\"' >> ~/.bashrc"
                    ;;
            esac
            echo ""
            ;;
    esac
}

detect_target() {
    local _os _arch _target _musl

    _os="$(uname -s)"
    _arch="$(uname -m)"

    case "$_os" in
        Darwin)
            case "$_arch" in
                x86_64)
                    # Check for Rosetta 2 — prefer native arm64 binary.
                    if [ "$(sysctl -n sysctl.proc_translated 2>/dev/null || echo 0)" = "1" ]; then
                        _target="aarch64-apple-darwin"
                    else
                        _target="x86_64-apple-darwin"
                    fi
                    ;;
                arm64) _target="aarch64-apple-darwin" ;;
                *) err "Unsupported architecture: $_arch" ;;
            esac
            ;;
        Linux)
            _musl=""
            if command -v ldd >/dev/null 2>&1; then
                if ldd --version 2>&1 | grep -qi musl; then
                    _musl="-musl"
                fi
            elif [ -f /etc/alpine-release ]; then
                _musl="-musl"
            fi

            local _libc="gnu"
            if [ -n "$_musl" ]; then _libc="musl"; fi

            case "$_arch" in
                x86_64 | amd64) _target="x86_64-unknown-linux-${_libc}" ;;
                aarch64 | arm64) _target="aarch64-unknown-linux-${_libc}" ;;
                *) err "Unsupported architecture: $_arch" ;;
            esac
            ;;
        *) err "Unsupported OS: $_os. Install via npm instead: npm install -g zerobox" ;;
    esac

    echo "$_target"
}

download() {
    local _url="$1" _output="$2"

    if command -v curl >/dev/null 2>&1; then
        curl --fail --location --silent --show-error --output "$_output" "$_url" ||
            err "Failed to download $_url"
    elif command -v wget >/dev/null 2>&1; then
        wget --quiet --output-document="$_output" "$_url" ||
            err "Failed to download $_url"
    else
        err "curl or wget is required to download zerobox"
    fi
}

log() {
    echo "  $1" >&2
}

err() {
    echo "error: $1" >&2
    exit 1
}

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        err "$1 is required but not found"
    fi
}

main "$@"
