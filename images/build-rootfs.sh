#!/usr/bin/env bash
set -euo pipefail

# Build a rootfs ext4 image from a Dockerfile
# Usage: ./build-rootfs.sh <image-name> [size-mib]
#
# Examples:
#   ./build-rootfs.sh base
#   ./build-rootfs.sh node24
#   ./build-rootfs.sh python313 4096

IMAGE_NAME="${1:?Usage: $0 <image-name> [size-mib]}"
SIZE_MIB="${2:-2048}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${PROJECT_DIR}/data/images"
DOCKERFILE="${SCRIPT_DIR}/Dockerfile.${IMAGE_NAME}"

if [[ ! -f "$DOCKERFILE" ]]; then
    echo "Error: Dockerfile not found: $DOCKERFILE"
    exit 1
fi

echo "==> Building Docker image: zerobox-rootfs-${IMAGE_NAME}"
docker build -t "zerobox-rootfs-${IMAGE_NAME}" -f "$DOCKERFILE" "$SCRIPT_DIR"

echo "==> Creating container"
CONTAINER_ID=$(docker create "zerobox-rootfs-${IMAGE_NAME}")

echo "==> Exporting filesystem"
mkdir -p "$OUTPUT_DIR"
ROOTFS_TAR="${OUTPUT_DIR}/${IMAGE_NAME}.tar"
docker export "$CONTAINER_ID" > "$ROOTFS_TAR"
docker rm "$CONTAINER_ID"

echo "==> Injecting guest agent"
# If the guest agent binary exists, inject it
AGENT_BIN="${PROJECT_DIR}/target/release/zerobox-agent"
if [[ -f "$AGENT_BIN" ]]; then
    # Append agent to the tar
    AGENT_DIR=$(mktemp -d)
    mkdir -p "${AGENT_DIR}/usr/local/bin"
    cp "$AGENT_BIN" "${AGENT_DIR}/usr/local/bin/zerobox-agent"
    chmod +x "${AGENT_DIR}/usr/local/bin/zerobox-agent"

    # Create systemd service for the agent
    mkdir -p "${AGENT_DIR}/etc/systemd/system"
    cat > "${AGENT_DIR}/etc/systemd/system/zerobox-agent.service" << 'UNIT'
[Unit]
Description=Zerobox Guest Agent
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/zerobox-agent
Restart=always
RestartSec=1

[Install]
WantedBy=multi-user.target
UNIT

    # Create symlink for auto-start
    mkdir -p "${AGENT_DIR}/etc/systemd/system/multi-user.target.wants"
    ln -sf /etc/systemd/system/zerobox-agent.service \
        "${AGENT_DIR}/etc/systemd/system/multi-user.target.wants/zerobox-agent.service"

    tar -rf "$ROOTFS_TAR" -C "$AGENT_DIR" .
    rm -rf "$AGENT_DIR"
    echo "    Guest agent injected"
else
    echo "    Warning: Guest agent not found at $AGENT_BIN, skipping injection"
fi

echo "==> Creating ext4 filesystem (${SIZE_MIB} MiB)"
ROOTFS_EXT4="${OUTPUT_DIR}/${IMAGE_NAME}.ext4"
dd if=/dev/zero of="$ROOTFS_EXT4" bs=1M count="$SIZE_MIB" status=progress
mkfs.ext4 -F "$ROOTFS_EXT4"

echo "==> Mounting and populating"
MOUNT_DIR=$(mktemp -d)
sudo mount -o loop "$ROOTFS_EXT4" "$MOUNT_DIR"
sudo tar -xf "$ROOTFS_TAR" -C "$MOUNT_DIR"
sudo umount "$MOUNT_DIR"
rmdir "$MOUNT_DIR"

# Clean up tar
rm -f "$ROOTFS_TAR"

echo "==> Done: $ROOTFS_EXT4"
echo "    Size: $(du -h "$ROOTFS_EXT4" | cut -f1)"
