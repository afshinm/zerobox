from __future__ import annotations

from .options import CommandOutput


class SandboxCommandError(Exception):
    """Raised when a sandboxed command exits with a non-zero code."""

    def __init__(self, output: CommandOutput) -> None:
        message = (
            output.stderr.strip()
            or output.stdout.strip()
            or f"command exited with code {output.code}"
        )
        super().__init__(message)
        self.code: int = output.code
        self.stdout: str = output.stdout
        self.stderr: str = output.stderr
