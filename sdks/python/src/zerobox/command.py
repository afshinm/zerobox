from __future__ import annotations

import json
import subprocess
from typing import Any, Union

from .binary import _not_found_error
from .errors import SandboxCommandError
from .options import CommandOutput


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
        except FileNotFoundError as e:
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
