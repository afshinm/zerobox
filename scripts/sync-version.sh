#!/usr/bin/env bash
#
# Sync the version from packages/zerobox/package.json into Cargo.toml.
# Called by `pnpm run version` after changesets bumps JS versions.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$SCRIPT_DIR/.."

VERSION=$(node -p "require('$ROOT/packages/zerobox/package.json').version")

if [ -z "$VERSION" ] || [ "$VERSION" = "undefined" ]; then
  echo "error: could not read version from packages/zerobox/package.json"
  exit 1
fi

echo "Syncing version: $VERSION"

# Update the workspace.package version in Cargo.toml.
# Use awk to only replace within the [workspace.package] section,
# avoiding false matches in [workspace.dependencies] etc.
awk -v ver="$VERSION" '
  /^\[workspace\.package\]/ { in_section=1 }
  /^\[/ && !/^\[workspace\.package\]/ { in_section=0 }
  in_section && /^version = / { $0 = "version = \"" ver "\"" }
  { print }
' "$ROOT/Cargo.toml" > "$ROOT/Cargo.toml.tmp" && mv "$ROOT/Cargo.toml.tmp" "$ROOT/Cargo.toml"

# Update workspace dependency version pins (version = "=x.y.z") to match.
if sed --version >/dev/null 2>&1; then
  sed -i "s/version = \"=[0-9.]*\"/version = \"=$VERSION\"/g" "$ROOT/Cargo.toml"
else
  sed -i '' "s/version = \"=[0-9.]*\"/version = \"=$VERSION\"/g" "$ROOT/Cargo.toml"
fi

echo "Cargo.toml workspace version:"
grep '^version' "$ROOT/Cargo.toml" | head -1

echo "Done."
