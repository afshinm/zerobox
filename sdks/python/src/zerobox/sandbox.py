from __future__ import annotations

from types import TracebackType
from typing import Any, Union

from .binary import async_verify_binary, verify_binary
from .command import AsyncShellCommand, ShellCommand
from .flags import build_flags
from .options import SandboxOptions, normalize_options


class Sandbox:
    """Sandbox for running commands. Construct via `Sandbox.create()`."""

    def __init__(self, options: SandboxOptions, bin_path: str) -> None:
        self.options = options
        self._bin = bin_path
        self._flags = build_flags(options)

    @classmethod
    def create(cls, options: Union[SandboxOptions, dict[str, Any], None] = None) -> Sandbox:
        """Create a Sandbox. Probes the binary once; raises if unreachable."""
        bin_path = verify_binary()
        return cls(normalize_options(options), bin_path)

    def sh(self, command: str) -> ShellCommand:
        return ShellCommand(self._bin, self._flags, "sh", ["-c", command])

    def py(self, code: str) -> ShellCommand:
        """Run `python3 -c <code>` using whichever `python3` is on PATH
        inside the sandbox. If that interpreter lives outside the readable
        roots (e.g. uv-managed Pythons), fall back to
        `.exec(sys.executable, ["-c", code])` with `allow_read=[sys.prefix]`.
        """
        return ShellCommand(self._bin, self._flags, "python3", ["-c", code])

    def exec(self, cmd: str, args: Union[list[str], None] = None) -> ShellCommand:
        return ShellCommand(self._bin, self._flags, cmd, list(args or []))

    def __enter__(self) -> Sandbox:
        return self

    def __exit__(
        self,
        exc_type: Union[type[BaseException], None],
        exc: Union[BaseException, None],
        tb: Union[TracebackType, None],
    ) -> None:
        pass


class AsyncSandbox:
    """Async sandbox for running commands. Construct via `await AsyncSandbox.create()`."""

    def __init__(self, options: SandboxOptions, bin_path: str) -> None:
        self.options = options
        self._bin = bin_path
        self._flags = build_flags(options)

    @classmethod
    async def create(
        cls, options: Union[SandboxOptions, dict[str, Any], None] = None
    ) -> AsyncSandbox:
        """Create an AsyncSandbox. Probes the binary once without blocking the event loop."""
        bin_path = await async_verify_binary()
        return cls(normalize_options(options), bin_path)

    def sh(self, command: str) -> AsyncShellCommand:
        return AsyncShellCommand(self._bin, self._flags, "sh", ["-c", command])

    def py(self, code: str) -> AsyncShellCommand:
        """Run `python3 -c <code>` using whichever `python3` is on PATH inside the sandbox."""
        return AsyncShellCommand(self._bin, self._flags, "python3", ["-c", code])

    def exec(self, cmd: str, args: Union[list[str], None] = None) -> AsyncShellCommand:
        return AsyncShellCommand(self._bin, self._flags, cmd, list(args or []))

    async def __aenter__(self) -> AsyncSandbox:
        return self

    async def __aexit__(
        self,
        exc_type: Union[type[BaseException], None],
        exc: Union[BaseException, None],
        tb: Union[TracebackType, None],
    ) -> None:
        pass
