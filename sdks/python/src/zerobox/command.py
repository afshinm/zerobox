from __future__ import annotations

import asyncio
import contextlib
import json
import locale
import subprocess
from typing import Any, Union

from .binary import _kill_process, _not_found_error
from .errors import SandboxCommandError
from .options import CommandOutput


def _decode(data: Union[bytes, None]) -> str:
    if not data:
        return ""
    return data.decode(locale.getpreferredencoding(False))


async def _communicate(
    proc: asyncio.subprocess.Process,
    command: list[str],
    timeout: Union[float, None],
) -> tuple[Union[bytes, None], Union[bytes, None]]:
    communicate_task = asyncio.create_task(proc.communicate())
    try:
        return await asyncio.wait_for(asyncio.shield(communicate_task), timeout=timeout)
    except asyncio.CancelledError:
        _kill_process(proc)
        with contextlib.suppress(Exception, asyncio.CancelledError):
            await communicate_task
        raise
    except asyncio.TimeoutError as e:
        _kill_process(proc)
        stdout, stderr = await communicate_task
        assert timeout is not None
        raise subprocess.TimeoutExpired(
            command,
            timeout,
            output=stdout,
            stderr=stderr,
        ) from e


class ShellCommand:
    """A pending command. Terminate with `.text()`, `.json()`, or `.output()`."""

    def __init__(self, bin_path: str, flags: list[str], cmd: str, args: list[str]) -> None:
        self._bin = bin_path
        self._argv = [*flags, "--", cmd, *args]

    def _run(self, *, timeout: Union[float, None] = None) -> CommandOutput:
        try:
            result = subprocess.run(
                [self._bin, *self._argv],
                capture_output=True,
                text=True,
                timeout=timeout,
                check=False,
            )
        except (FileNotFoundError, PermissionError) as e:
            raise _not_found_error(self._bin) from e
        return CommandOutput(
            code=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )

    def text(self, *, timeout: Union[float, None] = None) -> str:
        """stdout on success; raises SandboxCommandError on non-zero exit."""
        result = self._run(timeout=timeout)
        if result.code != 0:
            raise SandboxCommandError(result)
        return result.stdout

    def json(self, *, timeout: Union[float, None] = None) -> Any:
        """Parsed stdout on success; raises SandboxCommandError on non-zero exit."""
        return json.loads(self.text(timeout=timeout))

    def output(self, *, timeout: Union[float, None] = None) -> CommandOutput:
        """Raw `{code, stdout, stderr}`. Never raises on non-zero exit."""
        return self._run(timeout=timeout)


class AsyncShellCommand:
    """Async pending command.

    Terminate with `await .text()`, `await .json()`, or `await .output()`.
    """

    def __init__(self, bin_path: str, flags: list[str], cmd: str, args: list[str]) -> None:
        self._bin = bin_path
        self._argv = [*flags, "--", cmd, *args]

    async def _run(self, *, timeout: Union[float, None] = None) -> CommandOutput:
        command = [self._bin, *self._argv]
        try:
            proc = await asyncio.create_subprocess_exec(
                *command,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
            )
        except (FileNotFoundError, PermissionError) as e:
            raise _not_found_error(self._bin) from e

        stdout, stderr = await _communicate(proc, command, timeout)

        return CommandOutput(
            code=proc.returncode if proc.returncode is not None else 1,
            stdout=_decode(stdout),
            stderr=_decode(stderr),
        )

    async def text(self, *, timeout: Union[float, None] = None) -> str:
        """stdout on success; raises SandboxCommandError on non-zero exit."""
        result = await self._run(timeout=timeout)
        if result.code != 0:
            raise SandboxCommandError(result)
        return result.stdout

    async def json(self, *, timeout: Union[float, None] = None) -> Any:
        """Parsed stdout on success; raises SandboxCommandError on non-zero exit."""
        return json.loads(await self.text(timeout=timeout))

    async def output(self, *, timeout: Union[float, None] = None) -> CommandOutput:
        """Raw `{code, stdout, stderr}`. Never raises on non-zero exit."""
        return await self._run(timeout=timeout)
