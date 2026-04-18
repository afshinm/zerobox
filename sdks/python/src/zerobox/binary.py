from __future__ import annotations

import os
import shutil
import subprocess
import sysconfig
from pathlib import Path


def _not_found_error(bin_path: str) -> FileNotFoundError:
    return FileNotFoundError(
        f'zerobox binary not found at "{bin_path}". Run "pip install zerobox" or set ZEROBOX_BIN.'
    )


def resolve_binary() -> str:
    """Resolve the path to the zerobox binary.

    Order:
      1. ZEROBOX_BIN env var (explicit override).
      2. sysconfig scripts dir — where `pip install zerobox` put the binary via
         the wheel's shared_scripts entry. Works even when bin/ isn't on PATH.
      3. `shutil.which("zerobox")` — PATH lookup.
      4. Literal "zerobox" — final fallback; subprocess will raise if unresolved.
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
    """Resolve and probe the binary. Raises FileNotFoundError if it can't be run."""
    bin_path = resolve_binary()
    try:
        subprocess.run(
            [bin_path, "--help"],
            capture_output=True,
            check=False,
            timeout=5,
        )
    except FileNotFoundError as e:
        raise _not_found_error(bin_path) from e
    except subprocess.TimeoutExpired:
        pass
    return bin_path
