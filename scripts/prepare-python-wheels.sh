#!/usr/bin/env bash
#
# Adapter between cargo-dist release artifacts and the Python wheel builder.
#
# Usage: ./scripts/prepare-python-wheels.sh <artifacts-dir> <out-dir>
#
# Expects cargo-dist archives in <artifacts-dir>/ and lays out the binaries as
# <out-dir>/<target>/zerobox so sdks/python/scripts/build.sh can find them.

set -euo pipefail

ARTIFACTS="${1:?usage: $0 <artifacts-dir> <out-dir>}"
OUT="${2:?usage: $0 <artifacts-dir> <out-dir>}"

mkdir -p "$OUT"

TARGETS=(
    aarch64-apple-darwin
    x86_64-apple-darwin
    aarch64-unknown-linux-gnu
    x86_64-unknown-linux-gnu
    aarch64-unknown-linux-musl
    x86_64-unknown-linux-musl
)

for target in "${TARGETS[@]}"; do
    archive=$(find "$ARTIFACTS" -name "zerobox*${target}.tar.xz" -o -name "zerobox*${target}.tar.gz" | head -1)

    if [ -z "$archive" ]; then
        echo "warning: no archive found for $target, skipping"
        continue
    fi

    echo "Extracting $target from $(basename "$archive")"

    WORK_DIR=$(mktemp -d)
    if [[ "$archive" == *.tar.xz ]]; then
        tar xf "$archive" -C "$WORK_DIR"
    else
        tar xzf "$archive" -C "$WORK_DIR"
    fi

    bin=$(find "$WORK_DIR" -name "zerobox" -type f | head -1)
    if [ -z "$bin" ]; then
        echo "error: zerobox binary not found in $archive"
        rm -rf "$WORK_DIR"
        exit 1
    fi

    mkdir -p "$OUT/$target"
    cp "$bin" "$OUT/$target/zerobox"
    chmod +x "$OUT/$target/zerobox"
    rm -rf "$WORK_DIR"

    echo "  → $OUT/$target/zerobox"
done

echo "Done. Per-target binaries in $OUT."
