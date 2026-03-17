# Download a Firecracker-compatible kernel.
# Sourced by setup.sh — expects ARCH, KERNEL_DIR, ZEROBOX_USER.

install_kernel() {
    if [[ "${ZEROBOX_SKIP_KERNEL:-0}" == "1" ]]; then
        info "Skipping kernel (ZEROBOX_SKIP_KERNEL=1)"
        return 0
    fi

    local kernel_file="${KERNEL_DIR}/vmlinux-6.1-${ARCH}"
    if [[ -f "$kernel_file" ]]; then
        info "Kernel already present"
        return 0
    fi

    info "Downloading Firecracker kernel..."
    local url="https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide/${ARCH}/kernels/vmlinux.bin"
    curl -fSL -o "$kernel_file" "$url" || {
        warn "Failed to download kernel. You may need to build one manually."
        warn "See: kernels/download-kernels.sh"
        return 0
    }

    chmod 644 "$kernel_file"
    chown "${ZEROBOX_USER}:${ZEROBOX_USER}" "$kernel_file"
    info "Installed kernel"
}
