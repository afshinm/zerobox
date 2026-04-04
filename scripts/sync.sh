#!/usr/bin/env bash
#
# Sync upstream Codex sandboxing crates from a specific release.
#
# Usage:
#   ./sync.sh                    # sync from the pinned ref in UPSTREAM_VERSION
#   ./sync.sh v0.1.2503262      # sync from a specific tag/branch/SHA

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/.."
UPSTREAM_DIR="$ROOT/upstream"
VERSION_FILE="$ROOT/UPSTREAM_VERSION"

if [ $# -ge 1 ]; then
    REF="$1"
else
    if [ ! -f "$VERSION_FILE" ]; then
        echo "error: no ref specified and no UPSTREAM_VERSION file found"
        echo "usage: $0 <release-tag|branch|SHA>"
        exit 1
    fi
    REF="$(head -1 "$VERSION_FILE" | tr -d '[:space:]')"
fi

echo "==> Syncing from openai/codex @ $REF"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

echo "==> Cloning (shallow) into $WORK_DIR ..."
git clone --depth 1 --branch "$REF" https://github.com/openai/codex.git "$WORK_DIR/codex" 2>&1 | tail -2

SRC="$WORK_DIR/codex/codex-rs"

if [ ! -d "$SRC" ]; then
    echo "error: $SRC does not exist. Is the ref correct?"
    exit 1
fi

# Record the actual commit SHA for reproducibility.
COMMIT_SHA="$(git -C "$WORK_DIR/codex" rev-parse HEAD)"
echo "==> Resolved to commit $COMMIT_SHA"

CRATES=(
    sandboxing
    linux-sandbox
    windows-sandbox-rs
    process-hardening
    protocol
    execpolicy
    network-proxy
)

UTILS=(
    absolute-path
    string
    pty
    image
    cache
    template
    home-dir
    rustls-provider
)

echo "==> Cleaning upstream/"
rm -rf "$UPSTREAM_DIR"
mkdir -p "$UPSTREAM_DIR/utils"

for crate in "${CRATES[@]}"; do
    echo "    copying $crate/"
    cp -r "$SRC/$crate" "$UPSTREAM_DIR/$crate"
done

for util in "${UTILS[@]}"; do
    echo "    copying utils/$util/"
    cp -r "$SRC/utils/$util" "$UPSTREAM_DIR/utils/$util"
done

# Vendor directory (bubblewrap C sources for Linux)
if [ -d "$SRC/vendor" ]; then
    echo "    copying vendor/"
    cp -r "$SRC/vendor" "$UPSTREAM_DIR/vendor"
fi

echo "==> Applying patches..."

# windows-sandbox-rs uses path-based dep instead of workspace.
WIN_TOML="$UPSTREAM_DIR/windows-sandbox-rs/Cargo.toml"
if [ -f "$WIN_TOML" ] && grep -q 'path = "\.\./protocol"' "$WIN_TOML"; then
    echo "    patching windows-sandbox-rs/Cargo.toml (path dep -> workspace)"
    # Replace the multi-line [dependencies.codex-protocol] section with a single workspace line.
    sed -i.bak 's|\[dependencies\.codex-protocol\]|codex-protocol = { workspace = true }|' "$WIN_TOML"
    sed -i.bak '/^package = "codex-protocol"/d' "$WIN_TOML"
    sed -i.bak '/^path = "\.\.\/protocol"/d' "$WIN_TOML"
    rm -f "$WIN_TOML.bak"

    # Verify the patch took effect.
    if grep -q 'path = "\.\./protocol"' "$WIN_TOML"; then
        echo "error: failed to patch $WIN_TOML"
        exit 1
    fi
fi

# These add the RequestHeaderTransformer hook for secret substitution
# and enable MITM in Full mode. See the patch file for details.
PATCH="$SCRIPT_DIR/upstream-secret-substitution.patch"
if [ -f "$PATCH" ]; then
    echo "==> Applying zerobox upstream patches..."
    cd "$ROOT"
    patch -p1 < "$PATCH"
    # Format patched files to match rustfmt expectations.
    if command -v cargo >/dev/null 2>&1 && command -v rustfmt >/dev/null 2>&1; then
        echo "    formatting patched files"
        cargo fmt -- \
            upstream/network-proxy/src/certs.rs \
            upstream/network-proxy/src/http_proxy.rs \
            upstream/network-proxy/src/lib.rs \
            upstream/network-proxy/src/mitm.rs \
            upstream/network-proxy/src/runtime.rs \
            2>/dev/null || true
    fi
    cd -
fi

# Move non-FS platform defaults from restricted_read_only_platform_defaults.sbpl
# into the base policy. Profiles now control all filesystem rules.
PLATFORM_PATCH="$SCRIPT_DIR/upstream-platform-defaults.patch"
if [ -f "$PLATFORM_PATCH" ]; then
    echo "==> Applying platform defaults patch..."
    cd "$ROOT"
    patch -p0 < "$PLATFORM_PATCH"
    cd -
fi

{
    echo "$REF"
    echo "# commit: $COMMIT_SHA"
    echo "# synced: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$VERSION_FILE"

echo "==> Done. Synced to $REF ($COMMIT_SHA)"
