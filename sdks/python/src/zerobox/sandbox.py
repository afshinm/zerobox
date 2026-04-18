from __future__ import annotations

from types import TracebackType
from typing import Any, Union

from .binary import verify_binary
from .command import ShellCommand
from .flags import build_flags
from .options import SandboxOptions, normalize_options


class Sandbox:
    """Main entry point. Create with Sandbox.create()."""

    def __init__(self, options: SandboxOptions, bin_path: str) -> None:
        self.options = options
        self._bin = bin_path
        self._flags = build_flags(options)

    @classmethod
    def create(cls, options: Union[SandboxOptions, dict[str, Any], None] = None) -> Sandbox:
        """Create a Sandbox. Verifies the binary is reachable."""
        bin_path = verify_binary()
        return cls(normalize_options(options), bin_path)

    def sh(self, command: str) -> ShellCommand:
        """Run `sh -c <command>` in the sandbox."""
        return ShellCommand(self._bin, self._flags, "sh", ["-c", command])

    def py(self, code: str) -> ShellCommand:
        """Run `python3 -c <code>` in the sandbox."""
        return ShellCommand(self._bin, self._flags, "python3", ["-c", code])

    def exec(self, cmd: str, args: Union[list[str], None] = None) -> ShellCommand:
        """Run `cmd [args...]` in the sandbox."""
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
