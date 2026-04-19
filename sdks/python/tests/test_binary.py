from __future__ import annotations

import os
import stat
from pathlib import Path

import pytest

from zerobox import Sandbox, verify_binary


def _make_non_executable(tmp_path: Path) -> str:
    path = tmp_path / "fake-zerobox"
    path.write_text("#!/bin/sh\necho ok\n")
    path.chmod(stat.S_IRUSR | stat.S_IRGRP | stat.S_IROTH)  # readable but NOT executable
    return str(path)


def _make_missing(tmp_path: Path) -> str:
    return str(tmp_path / "does-not-exist" / "zerobox")


@pytest.mark.parametrize(
    ("case", "make"),
    [
        ("missing", _make_missing),
        ("not_executable", _make_non_executable),
    ],
)
def test_verify_binary_raises_on_unreachable(monkeypatch, tmp_path, case, make):
    monkeypatch.setenv("ZEROBOX_BIN", make(tmp_path))
    with pytest.raises(FileNotFoundError) as exc:
        verify_binary()
    msg = str(exc.value)
    assert "not found or not executable" in msg
    assert "ZEROBOX_BIN" in msg


def test_sandbox_run_wraps_permission_error(monkeypatch, tmp_path):
    """If the binary becomes non-executable between create() and run(), the
    user sees our friendly error, not a raw PermissionError."""
    bin_path = tmp_path / "zerobox"
    bin_path.write_text("#!/bin/sh\nexit 0\n")
    bin_path.chmod(0o755)
    monkeypatch.setenv("ZEROBOX_BIN", str(bin_path))

    sandbox = Sandbox.create()

    bin_path.chmod(stat.S_IRUSR)

    with pytest.raises(FileNotFoundError) as exc:
        sandbox.sh("echo hi").text()
    assert "not found or not executable" in str(exc.value)

    os.chmod(str(bin_path), 0o755)
