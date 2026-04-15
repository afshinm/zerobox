#!/usr/bin/env bash
#
# Sync sandbox crates from openai/codex, rename to zerobox-*.
#
# Usage:
#   ./sync.sh                    # use pinned ref from UPSTREAM_VERSION
#   ./sync.sh rust-v0.118.0      # specific tag/branch/SHA

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/.."

if sed --version >/dev/null 2>&1; then
    SED_INPLACE=(sed -i)
else
    SED_INPLACE=(sed -i '')
fi

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

COMMIT_SHA="$(git -C "$WORK_DIR/codex" rev-parse HEAD)"
echo "==> Resolved to commit $COMMIT_SHA"

CRATES=(
    sandboxing
    linux-sandbox
    windows-sandbox-rs
    process-hardening
    network-proxy
)

UTILS=(
    absolute-path
    string
    pty
    rustls-provider
)

echo "==> Cleaning upstream/"
rm -rf "$UPSTREAM_DIR"
mkdir -p "$UPSTREAM_DIR/utils"

for crate in "${CRATES[@]}"; do
    echo "    $crate/"
    cp -r "$SRC/$crate" "$UPSTREAM_DIR/$crate"
done

for util in "${UTILS[@]}"; do
    echo "    utils/$util/"
    cp -r "$SRC/utils/$util" "$UPSTREAM_DIR/utils/$util"
done

if [ -d "$SRC/vendor" ]; then
    echo "    vendor/"
    cp -r "$SRC/vendor" "$UPSTREAM_DIR/vendor"
fi

# --- Inline error types into linux-sandbox (replace codex-core dep) ---

echo "==> Patching linux-sandbox..."

rm -rf "$UPSTREAM_DIR/linux-sandbox/tests"

cat > "$UPSTREAM_DIR/linux-sandbox/src/error.rs" <<'ERRS'
use std::io;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, CodexErr>;

#[derive(Error, Debug)]
pub enum SandboxErr {
    #[cfg(target_os = "linux")]
    #[error("seccomp setup error")]
    SeccompInstall(#[from] seccompiler::Error),

    #[cfg(target_os = "linux")]
    #[error("seccomp backend error")]
    SeccompBackend(#[from] seccompiler::BackendError),

    #[error("command was killed by a signal")]
    Signal(i32),

    #[error("Landlock was not able to fully enforce all sandbox rules")]
    LandlockRestrict,
}

#[derive(Error, Debug)]
pub enum CodexErr {
    #[error("sandbox error: {0}")]
    Sandbox(#[from] SandboxErr),

    #[error("unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[cfg(target_os = "linux")]
    #[error(transparent)]
    LandlockRuleset(#[from] landlock::RulesetError),

    #[cfg(target_os = "linux")]
    #[error(transparent)]
    LandlockPathFd(#[from] landlock::PathFdError),
}
ERRS

"${SED_INPLACE[@]}" '/^#\[cfg(target_os = "linux")\]/{
N
/mod bwrap;/{
a\
#[cfg(target_os = "linux")]\
pub mod error;
}
}' "$UPSTREAM_DIR/linux-sandbox/src/lib.rs"

find "$UPSTREAM_DIR/linux-sandbox/src" -name '*.rs' -exec "${SED_INPLACE[@]}" \
    -e 's/use codex_core::error::/use crate::error::/g' \
    {} +

"${SED_INPLACE[@]}" '/^codex-core = /d' "$UPSTREAM_DIR/linux-sandbox/Cargo.toml"
"${SED_INPLACE[@]}" '/^clap = /a\
thiserror = { workspace = true }
' "$UPSTREAM_DIR/linux-sandbox/Cargo.toml"

# --- Patch windows-sandbox-rs (path dep -> workspace) ---

WIN_TOML="$UPSTREAM_DIR/windows-sandbox-rs/Cargo.toml"
if [ -f "$WIN_TOML" ] && grep -q 'path = "\.\./protocol"' "$WIN_TOML"; then
    echo "==> Patching windows-sandbox-rs..."
    "${SED_INPLACE[@]}" 's|\[dependencies\.codex-protocol\]|codex-protocol = { workspace = true }|' "$WIN_TOML"
    "${SED_INPLACE[@]}" '/^package = "codex-protocol"/d' "$WIN_TOML"
    "${SED_INPLACE[@]}" '/^path = "\.\.\/protocol"/d' "$WIN_TOML"
fi

# --- Rename codex-* → zerobox-* ---

echo "==> Renaming codex-* → zerobox-*..."

RENAME_PAIRS=(
    "codex-linux-sandbox:zerobox-linux-sandbox"
    "codex-network-proxy:zerobox-network-proxy"
    "codex-process-hardening:zerobox-process-hardening"
    "codex-protocol:zerobox-protocol"
    "codex-sandboxing:zerobox-sandboxing"
    "codex-windows-sandbox:zerobox-windows-sandbox"
    "codex-utils-absolute-path:zerobox-utils-absolute-path"
    "codex-utils-pty:zerobox-utils-pty"
    "codex-utils-string:zerobox-utils-string"
    "codex-utils-home-dir:zerobox-utils-home-dir"
    "codex-utils-rustls-provider:zerobox-utils-rustls-provider"
    "codex-command-runner:zerobox-command-runner"
    "codex-windows-sandbox-setup:zerobox-windows-sandbox-setup"
    "find-codex-home:find-home"
)

SED_ARGS=()
for pair in "${RENAME_PAIRS[@]}"; do
    old="${pair%%:*}"
    new="${pair##*:}"
    SED_ARGS+=(-e "s/${old}/${new}/g")
    old_us="${old//-/_}"
    new_us="${new//-/_}"
    SED_ARGS+=(-e "s/${old_us}/${new_us}/g")
done

find "$UPSTREAM_DIR" \( -name '*.rs' -o -name '*.toml' \) \
    -exec "${SED_INPLACE[@]}" "${SED_ARGS[@]}" {} +

find "$UPSTREAM_DIR" -name '*.rs' -exec "${SED_INPLACE[@]}" \
    -e 's/CODEX_LINUX_SANDBOX_ARG0/ZEROBOX_LINUX_SANDBOX_ARG0/g' \
    {} +

# Add workspace metadata inheritance for crates.io publishing.
find "$UPSTREAM_DIR" -name 'Cargo.toml' -exec "${SED_INPLACE[@]}" \
    '/^license.workspace/a\
description.workspace = true\
repository.workspace = true\
homepage.workspace = true
' {} +

# --- Apply patches ---

echo "==> Applying patches..."

cd "$ROOT"

PATCH="$SCRIPT_DIR/upstream-secret-substitution.patch"
if [ -f "$PATCH" ]; then
    echo "    secret-substitution"
    patch -p1 < "$PATCH"
    if command -v cargo >/dev/null 2>&1 && command -v rustfmt >/dev/null 2>&1; then
        cargo fmt -- \
            upstream/network-proxy/src/certs.rs \
            upstream/network-proxy/src/http_proxy.rs \
            upstream/network-proxy/src/lib.rs \
            upstream/network-proxy/src/mitm.rs \
            upstream/network-proxy/src/runtime.rs \
            2>/dev/null || true
    fi
fi

PLATFORM_PATCH="$SCRIPT_DIR/upstream-platform-defaults.patch"
if [ -f "$PLATFORM_PATCH" ]; then
    echo "    platform-defaults"
    patch -p0 < "$PLATFORM_PATCH"
fi

DENY_WRITE_PATCH="$SCRIPT_DIR/upstream-deny-default-write.patch"
if [ -f "$DENY_WRITE_PATCH" ]; then
    echo "    deny-default-write"
    patch -p0 < "$DENY_WRITE_PATCH"
fi

CODEX_PROTECT_PATCH="$SCRIPT_DIR/upstream-no-preemptive-codex-protect.patch"
if [ -f "$CODEX_PROTECT_PATCH" ]; then
    echo "    no-preemptive-codex-protect"
    patch -p0 < "$CODEX_PROTECT_PATCH"
    if command -v cargo >/dev/null 2>&1 && command -v rustfmt >/dev/null 2>&1; then
        cargo fmt -- \
            upstream/sandboxing/src/seatbelt_tests.rs \
            upstream/linux-sandbox/src/bwrap.rs \
            2>/dev/null || true
    fi
fi

HOME_ENV_PATCH="$SCRIPT_DIR/upstream-zerobox-home-env.patch"
if [ -f "$HOME_ENV_PATCH" ]; then
    echo "    zerobox-home-env"
    patch -p0 < "$HOME_ENV_PATCH"
fi

NODE_PROXY_PATCH="$SCRIPT_DIR/upstream-node-env-proxy.patch"
if [ -f "$NODE_PROXY_PATCH" ]; then
    echo "    node-env-proxy"
    patch -p0 < "$NODE_PROXY_PATCH"
fi

cd -

{
    echo "$REF"
    echo "# commit: $COMMIT_SHA"
    echo "# synced: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
} > "$VERSION_FILE"

echo "==> Done. Synced to $REF ($COMMIT_SHA)"
