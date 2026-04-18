#!/usr/bin/env bash
#
# Build zerobox wheels for PyPI.
#
# Usage: ./scripts/build.sh <artifacts-dir>
#
# <artifacts-dir> must contain one directory per cargo target, each holding a
# `zerobox` binary:
#
#   <artifacts-dir>/aarch64-apple-darwin/zerobox
#   <artifacts-dir>/x86_64-apple-darwin/zerobox
#   <artifacts-dir>/aarch64-unknown-linux-gnu/zerobox
#   ...
#
# Output: one sdist + six platform wheels in dist/.

set -euo pipefail

ARTIFACTS_DIR="${1:?usage: $0 <artifacts-dir>}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PKG_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Resolve to absolute before cd-ing.
if [[ "$ARTIFACTS_DIR" != /* ]]; then
    ARTIFACTS_DIR="$(cd "$ARTIFACTS_DIR" && pwd)"
fi

cd "$PKG_ROOT"
rm -rf dist

echo "==> Building sdist"
uv build --sdist

TARGETS=(
    aarch64-apple-darwin
    x86_64-apple-darwin
    aarch64-unknown-linux-gnu
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-musl
    x86_64-unknown-linux-musl
)

for target in "${TARGETS[@]}"; do
    binary="$ARTIFACTS_DIR/$target/zerobox"
    if [ ! -f "$binary" ]; then
        echo "error: missing binary for $target at $binary"
        exit 1
    fi
    echo "==> Building wheel for $target"
    ZEROBOX_ARTIFACTS_DIR="$ARTIFACTS_DIR" \
    ZEROBOX_WHEEL_TARGET="$target" \
        uv build --wheel
done

echo "==> Artifacts in $PKG_ROOT/dist:"
ls -la dist/
