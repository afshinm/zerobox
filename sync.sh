#!/usr/bin/env bash
#
# Sync upstream Codex sandboxing crates from a specific release.
#
# Usage:
#   ./sync.sh                    # sync from the pinned ref in UPSTREAM_VERSION
#   ./sync.sh v0.1.2503262      # sync from a specific tag/branch/SHA
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
UPSTREAM_DIR="$SCRIPT_DIR/upstream"
VERSION_FILE="$SCRIPT_DIR/UPSTREAM_VERSION"

# ── Resolve version ──
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

# ── Clone into temp dir (avoid shadowing system TMPDIR) ──
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

# ── Crates to copy ──
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

# ── Clean and copy ──
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

# ── Apply minimal patches ──
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

# ── Save version ──
{
    echo "$REF"
    echo "# commit: $COMMIT_SHA"
    echo "# synced: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$VERSION_FILE"

echo "==> Done. Synced to $REF ($COMMIT_SHA)"
echo ""
echo "Next steps:"
echo "  cd $(basename "$SCRIPT_DIR") && cargo check"
echo "  If it fails, update shims/ to match any API changes."
