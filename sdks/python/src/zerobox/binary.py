from __future__ import annotations

import asyncio
import os
import shutil
import subprocess
import sysconfig
from pathlib import Path

from ._process import kill_process


def _not_found_error(bin_path: str) -> FileNotFoundError:
    return FileNotFoundError(
        f'zerobox binary at "{bin_path}" not found or not executable. '
        'Run "pip install zerobox" or set ZEROBOX_BIN.'
    )


def resolve_binary() -> str:
    """Resolve the path to the zerobox binary.

    Order.
      1. ZEROBOX_BIN env var (explicit override).
      2. sysconfig scripts dir, where `pip install zerobox` puts the binary
         via the wheel's shared_scripts entry. Works even when bin/ isn't on
         PATH.
      3. `shutil.which("zerobox")` as a PATH lookup.
      4. Literal "zerobox" as a final fallback; subprocess will raise if
         unresolved.
    """
    override = os.environ.get("ZEROBOX_BIN")
    if override:
        return override

    scripts_dir = sysconfig.get_path("scripts")
    if scripts_dir:
        candidate = Path(scripts_dir) / "zerobox"
        if candidate.exists():
            return str(candidate)

    return shutil.which("zerobox") or "zerobox"


def verify_binary() -> str:
    """Resolve the binary and probe it. Raises FileNotFoundError if unreachable."""
    bin_path = resolve_binary()
    try:
        subprocess.run(
            [bin_path, "--help"],
            capture_output=True,
            check=False,
            timeout=5,
        )
    except (FileNotFoundError, PermissionError) as e:
        raise _not_found_error(bin_path) from e
    except subprocess.TimeoutExpired:
        pass
    return bin_path


async def async_verify_binary() -> str:
    """Async version of `verify_binary`.

    This avoids blocking the current event loop when async applications create a
    sandbox.
    """
    bin_path = resolve_binary()
    try:
        proc = await asyncio.create_subprocess_exec(
            bin_path,
            "--help",
            stdout=asyncio.subprocess.DEVNULL,
            stderr=asyncio.subprocess.DEVNULL,
        )
        try:
            await asyncio.wait_for(proc.wait(), timeout=5)
        except asyncio.CancelledError:
            kill_process(proc)
            await proc.wait()
            raise
        except asyncio.TimeoutError:
            kill_process(proc)
            await proc.wait()
    except (FileNotFoundError, PermissionError) as e:
        raise _not_found_error(bin_path) from e
    return bin_path
