from __future__ import annotations

import asyncio
import subprocess
import sys
import time
from pathlib import Path

import pytest

from zerobox import AsyncSandbox, Sandbox, SandboxCommandError


def _write_marker_after(marker: Path, seconds: float) -> str:
    return (
        "import pathlib, time; "
        f"time.sleep({seconds}); "
        f"pathlib.Path({str(marker)!r}).write_text('done')"
    )


@pytest.fixture
def fake_zerobox(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> str:
    bin_path = tmp_path / "zerobox"
    bin_path.write_text(
        "#!/usr/bin/env python3\n"
        "import subprocess\n"
        "import sys\n"
        "\n"
        "if sys.argv[1:] == ['--help']:\n"
        "    print('fake zerobox')\n"
        "    raise SystemExit(0)\n"
        "\n"
        "try:\n"
        "    sep = sys.argv.index('--')\n"
        "except ValueError:\n"
        "    raise SystemExit(2)\n"
        "\n"
        "cmd = sys.argv[sep + 1:]\n"
        "result = subprocess.run(cmd, check=False)\n"
        "raise SystemExit(result.returncode)\n"
    )
    bin_path.chmod(0o755)
    monkeypatch.setenv("ZEROBOX_BIN", str(bin_path))
    return str(bin_path)


def test_async_sh_text_returns_stdout(fake_zerobox: str) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        assert (await sandbox.sh("echo hello").text()).strip() == "hello"

    asyncio.run(run())


def test_async_sh_json_parses_stdout(fake_zerobox: str) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        data = await sandbox.sh('echo \'{"key":"value"}\'').json()
        assert data["key"] == "value"

    asyncio.run(run())


def test_async_output_never_raises(fake_zerobox: str) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        result = await sandbox.sh("exit 42").output()
        assert result.code == 42

    asyncio.run(run())


def test_async_text_raises_on_non_zero(fake_zerobox: str) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        with pytest.raises(SandboxCommandError) as exc:
            await sandbox.sh("exit 42").text()
        assert exc.value.code == 42

    asyncio.run(run())


def test_async_exec_captures_stdout_and_stderr(fake_zerobox: str) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        result = await sandbox.exec(
            sys.executable,
            ["-c", "import sys; print('out'); print('err', file=sys.stderr)"],
        ).output()
        assert result.code == 0
        assert result.stdout.strip() == "out"
        assert result.stderr.strip() == "err"

    asyncio.run(run())


def test_async_timeout_kills_command_spawned_by_wrapper(fake_zerobox: str, tmp_path: Path) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        marker = tmp_path / "timeout-child-marker"
        with pytest.raises(subprocess.TimeoutExpired):
            await sandbox.exec(
                sys.executable,
                ["-c", _write_marker_after(marker, 0.4)],
            ).text(timeout=0.1)
        await asyncio.sleep(0.5)
        assert not marker.exists()

    asyncio.run(run())


def test_sync_timeout_kills_command_spawned_by_wrapper(fake_zerobox: str, tmp_path: Path) -> None:
    sandbox = Sandbox.create()
    marker = tmp_path / "sync-timeout-child-marker"
    with pytest.raises(subprocess.TimeoutExpired):
        sandbox.exec(
            sys.executable,
            ["-c", _write_marker_after(marker, 0.4)],
        ).text(timeout=0.1)
    time.sleep(0.5)
    assert not marker.exists()


def test_async_timeout_preserves_partial_output(fake_zerobox: str) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        with pytest.raises(subprocess.TimeoutExpired) as exc:
            await sandbox.exec(
                sys.executable,
                ["-c", "import sys, time; print('before'); sys.stdout.flush(); time.sleep(10)"],
            ).text(timeout=0.1)
        assert exc.value.output is not None
        assert b"before" in exc.value.output

    asyncio.run(run())


def test_sync_timeout_preserves_partial_output(fake_zerobox: str) -> None:
    sandbox = Sandbox.create()
    with pytest.raises(subprocess.TimeoutExpired) as exc:
        sandbox.exec(
            sys.executable,
            ["-c", "import sys, time; print('before'); sys.stdout.flush(); time.sleep(10)"],
        ).text(timeout=0.1)
    assert exc.value.output is not None
    assert b"before" in exc.value.output


def test_async_cancellation_kills_command_spawned_by_wrapper(
    fake_zerobox: str, tmp_path: Path
) -> None:
    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        marker = tmp_path / "cancel-child-marker"
        task = asyncio.create_task(
            sandbox.exec(
                sys.executable,
                ["-c", _write_marker_after(marker, 0.4)],
            ).text()
        )
        await asyncio.sleep(0.1)
        task.cancel()
        with pytest.raises(asyncio.CancelledError):
            await task
        await asyncio.sleep(0.5)
        assert not marker.exists()

    asyncio.run(run())


def test_async_context_manager(fake_zerobox: str) -> None:
    async def run() -> None:
        async with await AsyncSandbox.create() as sandbox:
            assert (await sandbox.sh("echo hi").text()).strip() == "hi"

    asyncio.run(run())


def test_async_run_wraps_permission_error(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    bin_path = tmp_path / "zerobox"
    bin_path.write_text("#!/bin/sh\nexit 0\n")
    bin_path.chmod(0o755)
    monkeypatch.setenv("ZEROBOX_BIN", str(bin_path))

    async def run() -> None:
        sandbox = await AsyncSandbox.create()
        bin_path.chmod(0o444)

        with pytest.raises(FileNotFoundError) as exc:
            await sandbox.sh("echo hi").text()
        assert "not found or not executable" in str(exc.value)

    try:
        asyncio.run(run())
    finally:
        bin_path.chmod(0o755)


def test_async_create_wraps_missing_binary(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> None:
    monkeypatch.setenv("ZEROBOX_BIN", str(tmp_path / "missing" / "zerobox"))

    async def run() -> None:
        with pytest.raises(FileNotFoundError) as exc:
            await AsyncSandbox.create()
        assert "not found or not executable" in str(exc.value)

    asyncio.run(run())


def test_async_create_wraps_permission_error(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
) -> None:
    bin_path = tmp_path / "zerobox"
    bin_path.write_text("#!/bin/sh\nexit 0\n")
    bin_path.chmod(0o444)
    monkeypatch.setenv("ZEROBOX_BIN", str(bin_path))

    async def run() -> None:
        with pytest.raises(FileNotFoundError) as exc:
            await AsyncSandbox.create()
        assert "not found or not executable" in str(exc.value)

    try:
        asyncio.run(run())
    finally:
        bin_path.chmod(0o755)
