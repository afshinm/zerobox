"""Platform detection — diagnostic only.

Wheel tags pick the right binary at `pip install` time, so nothing in the runtime
path needs this. Kept for parity with the TypeScript SDK and as a debugging aid
(`python -m zerobox.platforms`).

Port of packages/zerobox/src/platforms.ts.
"""

from __future__ import annotations

import os
import platform as _platform
import subprocess
from collections.abc import Callable
from dataclasses import dataclass, field
from typing import Union

_GLIBC_PACKAGES: dict[str, str] = {
    "darwin-arm64": "zerobox-darwin-arm64",
    "darwin-x86_64": "zerobox-darwin-x86_64",
    "linux-arm64": "zerobox-linux-arm64",
    "linux-x86_64": "zerobox-linux-x86_64",
}

_MUSL_PACKAGES: dict[str, str] = {
    "linux-arm64": "zerobox-linux-arm64-musl",
    "linux-x86_64": "zerobox-linux-x86_64-musl",
}

_MUSL_LINKER: dict[str, str] = {
    "arm64": "/lib/ld-musl-aarch64.so.1",
    "x86_64": "/lib/ld-musl-x86_64.so.1",
}


def _normalize_arch(arch: str) -> str:
    arch = arch.lower()
    if arch in {"aarch64", "arm64"}:
        return "arm64"
    if arch in {"x86_64", "amd64", "x64"}:
        return "x86_64"
    return arch


def _default_linker_exists(path: str) -> bool:
    return os.path.exists(path)


def _default_libc_version() -> Union[str, None]:
    libc, version = _platform.libc_ver()
    if libc == "glibc" and version:
        return version
    return None


def _default_ldd_output() -> Union[str, None]:
    try:
        result = subprocess.run(
            ["ldd", "--version"],
            capture_output=True,
            text=True,
            timeout=3,
            check=False,
        )
    except (FileNotFoundError, subprocess.TimeoutExpired):
        return None
    return (result.stdout or "") + (result.stderr or "")


def _default_os_release() -> Union[str, None]:
    try:
        with open("/etc/os-release", encoding="utf-8") as f:
            return f.read()
    except OSError:
        return None


@dataclass
class PlatformEnv:
    """Dependencies for detection, injectable for testing."""

    platform: str = field(default_factory=lambda: _platform.system().lower())
    arch: str = field(default_factory=lambda: _normalize_arch(_platform.machine()))
    linker_exists: Callable[[str], bool] = field(default=_default_linker_exists)
    libc_version: Callable[[], Union[str, None]] = field(default=_default_libc_version)
    ldd_output: Callable[[], Union[str, None]] = field(default=_default_ldd_output)
    os_release: Callable[[], Union[str, None]] = field(default=_default_os_release)


def detect_musl(env: Union[PlatformEnv, None] = None) -> bool:
    """Detect whether the current Linux system uses musl libc.

    Four-tier ladder (fastest + most reliable first):
      1. musl dynamic linker file on disk.
      2. `platform.libc_ver()` reports a glibc version → not musl.
      3. `ldd --version` output contains "musl" / "gnu".
      4. /etc/os-release mentions Alpine.
    """
    e = env or PlatformEnv()
    if e.platform != "linux":
        return False

    linker = _MUSL_LINKER.get(e.arch)
    if linker and e.linker_exists(linker):
        return True

    if e.libc_version():
        return False

    ldd = e.ldd_output()
    if ldd:
        lower = ldd.lower()
        if "musl" in lower:
            return True
        if "gnu" in lower:
            return False

    os_release = e.os_release()
    return bool(os_release and "alpine" in os_release.lower())


def platform_package(env: Union[PlatformEnv, None] = None) -> Union[str, None]:
    """Platform label for the current system, or None. Diagnostic only."""
    e = env or PlatformEnv()
    key = f"{e.platform}-{e.arch}"
    if e.platform == "linux" and detect_musl(e):
        return _MUSL_PACKAGES.get(key) or _GLIBC_PACKAGES.get(key)
    return _GLIBC_PACKAGES.get(key)
