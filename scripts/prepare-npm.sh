#!/usr/bin/env bash
#
# Prepare npm platform packages from cargo-dist release artifacts.
#
# Usage: ./scripts/prepare-npm.sh <artifacts-dir>
#
# Expects cargo-dist archives in <artifacts-dir>/ matching:
#   zerobox-*-aarch64-apple-darwin.tar.xz
#   zerobox-*-x86_64-apple-darwin.tar.xz
#   zerobox-*-aarch64-unknown-linux-gnu.tar.xz
#   zerobox-*-x86_64-unknown-linux-gnu.tar.xz
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/.."
ARTIFACTS="${1:?usage: $0 <artifacts-dir>}"

# Map cargo-dist target triples to npm platform package dirs.
declare -A TARGET_MAP=(
  ["aarch64-apple-darwin"]="cli-darwin-arm64"
  ["x86_64-apple-darwin"]="cli-darwin-x64"
  ["aarch64-unknown-linux-gnu"]="cli-linux-arm64"
  ["x86_64-unknown-linux-gnu"]="cli-linux-x64"
  ["aarch64-unknown-linux-musl"]="cli-linux-arm64-musl"
  ["x86_64-unknown-linux-musl"]="cli-linux-x64-musl"
)

for target in "${!TARGET_MAP[@]}"; do
  pkg_dir="$ROOT/packages/${TARGET_MAP[$target]}"
  archive=$(find "$ARTIFACTS" -name "zerobox-*-${target}.tar.xz" -o -name "zerobox-*-${target}.tar.gz" | head -1)

  if [ -z "$archive" ]; then
    echo "warning: no archive found for $target, skipping"
    continue
  fi

  echo "Extracting $target from $(basename "$archive")"

  # Extract the zerobox binary from the archive.
  WORK_DIR=$(mktemp -d)
  if [[ "$archive" == *.tar.xz ]]; then
    tar xf "$archive" -C "$WORK_DIR"
  else
    tar xzf "$archive" -C "$WORK_DIR"
  fi

  # cargo-dist puts the binary inside a directory named after the archive.
  bin=$(find "$WORK_DIR" -name "zerobox" -type f | head -1)
  if [ -z "$bin" ]; then
    echo "error: zerobox binary not found in $archive"
    rm -rf "$WORK_DIR"
    exit 1
  fi

  cp "$bin" "$pkg_dir/zerobox"
  chmod +x "$pkg_dir/zerobox"
  rm -rf "$WORK_DIR"

  echo "  → ${TARGET_MAP[$target]}/zerobox"
done

echo "Done. Platform packages ready for publishing."
