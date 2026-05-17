from importlib.metadata import PackageNotFoundError, version

from .binary import async_verify_binary, resolve_binary, verify_binary
from .command import AsyncShellCommand, ShellCommand
from .errors import SandboxCommandError
from .flags import build_flags
from .options import CommandOutput, SandboxOptions, SecretConfig
from .sandbox import AsyncSandbox, Sandbox

try:
    __version__ = version("zerobox")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = [
    "AsyncSandbox",
    "AsyncShellCommand",
    "CommandOutput",
    "Sandbox",
    "SandboxCommandError",
    "SandboxOptions",
    "SecretConfig",
    "ShellCommand",
    "__version__",
    "async_verify_binary",
    "build_flags",
    "resolve_binary",
    "verify_binary",
]
