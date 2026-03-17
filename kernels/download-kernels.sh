#!/usr/bin/env bash
set -euo pipefail

# Download pre-built Linux kernels for Firecracker
# These are the official Firecracker CI kernels

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
KERNEL_DIR="${PROJECT_DIR}/data/kernels"

# Firecracker quickstart kernel images (from Firecracker's S3 bucket)
KERNEL_VERSION="6.1"
BASE_URL="https://s3.amazonaws.com/spec.ccfc.min/img/quickstart_guide"

mkdir -p "$KERNEL_DIR"

download_kernel() {
    local arch="$1"
    local filename="vmlinux-${KERNEL_VERSION}-${arch}"
    local output="${KERNEL_DIR}/${filename}"

    if [[ -f "$output" ]]; then
        echo "Kernel already exists: $output"
        return
    fi

    local url="${BASE_URL}/${arch}/kernels/vmlinux.bin"
    echo "==> Downloading kernel: ${filename}"
    echo "    URL: ${url}"
    curl -fSL "$url" -o "$output"
    chmod 644 "$output"
    echo "    Saved to: $output"
    echo "    Size: $(du -h "$output" | cut -f1)"
}

echo "Downloading Firecracker-compatible Linux kernels (v${KERNEL_VERSION})"
echo ""

# Detect current architecture
CURRENT_ARCH="$(uname -m)"

case "$CURRENT_ARCH" in
    x86_64)
        download_kernel "x86_64"
        echo ""
        echo "To also download the aarch64 kernel, run:"
        echo "  $0 --all"
        ;;
    aarch64)
        download_kernel "aarch64"
        echo ""
        echo "To also download the x86_64 kernel, run:"
        echo "  $0 --all"
        ;;
    *)
        echo "Unknown architecture: $CURRENT_ARCH"
        echo "Downloading both kernels..."
        download_kernel "x86_64"
        download_kernel "aarch64"
        ;;
esac

if [[ "${1:-}" == "--all" ]]; then
    download_kernel "x86_64"
    download_kernel "aarch64"
fi

echo ""
echo "==> Done. Kernels stored in: $KERNEL_DIR"
ls -lh "$KERNEL_DIR"/
