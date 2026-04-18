from importlib.metadata import PackageNotFoundError, version

from .binary import resolve_binary, verify_binary
from .command import ShellCommand
from .errors import SandboxCommandError
from .flags import build_flags
from .options import CommandOutput, SandboxOptions, SecretConfig
from .sandbox import Sandbox

try:
    __version__ = version("zerobox")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = [
    "CommandOutput",
    "Sandbox",
    "SandboxCommandError",
    "SandboxOptions",
    "SecretConfig",
    "ShellCommand",
    "__version__",
    "build_flags",
    "resolve_binary",
    "verify_binary",
]
