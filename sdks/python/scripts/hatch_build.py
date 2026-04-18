"""Custom hatchling build hook for zerobox wheels.

Pattern follows denoland/deno_pypi: env-var-driven platform selection, prebuilt
binary copied in at wheel-build time, wheel tag set from a target map.

Flow per wheel build:
  1. Read VERSION from ../zerobox/package.json (shared with npm + cargo).
  2. If building sdist: no-op, return.
  3. Read ZEROBOX_WHEEL_TARGET (e.g. "aarch64-apple-darwin") to pick a tag.
  4. Locate the prebuilt binary at ZEROBOX_ARTIFACTS_DIR/<target>/zerobox.
  5. Set build_data["tag"] so pip picks the right wheel for the installing host.
  6. Set build_data["shared_scripts"] so `pip install zerobox` drops the binary
     straight into the venv's bin/ (same as `pip install deno`).
"""

from __future__ import annotations

import json
import os
import re
from pathlib import Path

from hatchling.builders.hooks.plugin.interface import BuildHookInterface

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
REPO_ROOT = ROOT.parent.parent
NPM_PACKAGE_JSON = REPO_ROOT / "packages" / "zerobox" / "package.json"

TARGET_TAGS: dict[str, str] = {
    "aarch64-apple-darwin": "py3-none-macosx_11_0_arm64",
    "x86_64-apple-darwin": "py3-none-macosx_10_12_x86_64",
    "aarch64-unknown-linux-gnu": "py3-none-manylinux_2_17_aarch64",
    "x86_64-unknown-linux-gnu": "py3-none-manylinux_2_17_x86_64",
    "aarch64-unknown-linux-musl": "py3-none-musllinux_1_1_aarch64",
    "x86_64-unknown-linux-musl": "py3-none-musllinux_1_1_x86_64",
}


def _read_version() -> str:
    env_override = os.environ.get("ZEROBOX_VERSION")
    if env_override:
        return env_override
    try:
        data = json.loads(NPM_PACKAGE_JSON.read_text())
        return str(data["version"])
    except (OSError, KeyError, json.JSONDecodeError):
        return "0.0.0"


VERSION = _read_version()

_SEMVER = re.compile(r"^\d+\.\d+\.\d+([.-][A-Za-z0-9.]+)?$")
if not _SEMVER.match(VERSION):
    raise RuntimeError(
        f"zerobox version {VERSION!r} does not look like a valid semver. "
        f"Check {NPM_PACKAGE_JSON}."
    )


class ZeroboxBuildHook(BuildHookInterface):
    PLUGIN_NAME = "custom"

    def initialize(self, version: str, build_data: dict) -> None:
        # Editable installs (uv sync, pip install -e .) skip binary bundling —
        # developers use ZEROBOX_BIN or a local cargo build.
        if self.target_name != "wheel" or version == "editable":
            return

        target = os.environ.get("ZEROBOX_WHEEL_TARGET")
        if not target:
            raise RuntimeError(
                "ZEROBOX_WHEEL_TARGET is required to build a zerobox wheel. "
                f"Expected one of: {sorted(TARGET_TAGS)}"
            )

        tag = TARGET_TAGS.get(target)
        if tag is None:
            raise RuntimeError(
                f"Unknown ZEROBOX_WHEEL_TARGET={target!r}. "
                f"Expected one of: {sorted(TARGET_TAGS)}"
            )

        artifacts_dir = os.environ.get("ZEROBOX_ARTIFACTS_DIR")
        if not artifacts_dir:
            raise RuntimeError(
                "ZEROBOX_ARTIFACTS_DIR must point at a directory containing "
                "<target>/zerobox for each target being built."
            )

        binary = Path(artifacts_dir) / target / "zerobox"
        if not binary.exists():
            raise RuntimeError(f"prebuilt binary not found: {binary}")

        build_data["tag"] = tag
        build_data["shared_scripts"] = {str(binary): "zerobox"}
        build_data["pure_python"] = False
