from __future__ import annotations

from dataclasses import dataclass, field, fields
from typing import Any, Union


@dataclass(frozen=True)
class SecretConfig:
    value: str
    hosts: list[str] = field(default_factory=list)


@dataclass(frozen=True)
class BindMount:
    """A single host→sandbox path remapping.

    On Linux (and WSL2) the CLI performs a real bind mount; on macOS,
    Windows, and WSL1 the CLI emits a one-line warning to stderr and runs
    the command without remapping.
    """

    host: str
    sandbox: str
    read_only: bool = False


@dataclass(frozen=True)
class CommandOutput:
    code: int
    stdout: str
    stderr: str


@dataclass(frozen=True)
class SandboxOptions:
    profile: Union[str, list[str], None] = None
    allow_read: Union[list[str], None] = None
    deny_read: Union[list[str], None] = None
    allow_write: Union[list[str], None] = None
    deny_write: Union[list[str], None] = None
    allow_net: Union[bool, list[str], None] = None
    deny_net: Union[list[str], None] = None
    allow_all: bool = False
    no_sandbox: bool = False
    strict_sandbox: bool = False
    cwd: Union[str, None] = None
    env: Union[dict[str, str], None] = None
    allow_env: Union[bool, list[str], None] = None
    deny_env: Union[list[str], None] = None
    snapshot: bool = False
    restore: bool = False
    snapshot_paths: Union[list[str], None] = None
    snapshot_exclude: Union[list[str], None] = None
    secrets: Union[dict[str, SecretConfig], None] = None
    bind_mounts: Union[list[BindMount], None] = None
    debug: bool = False


def _coerce_secret(cfg: Union[SecretConfig, dict[str, Any]]) -> SecretConfig:
    if isinstance(cfg, SecretConfig):
        return cfg
    return SecretConfig(value=cfg["value"], hosts=list(cfg.get("hosts") or []))


def _coerce_bind_mount(mount: Union[BindMount, dict[str, Any]]) -> BindMount:
    if isinstance(mount, BindMount):
        return mount
    return BindMount(
        host=mount["host"],
        sandbox=mount["sandbox"],
        read_only=bool(mount.get("read_only", False)),
    )


def normalize_options(
    options: Union[SandboxOptions, dict[str, Any], None],
) -> SandboxOptions:
    if options is None:
        return SandboxOptions()
    if isinstance(options, SandboxOptions):
        return options

    valid = {f.name for f in fields(SandboxOptions)}
    unknown = set(options) - valid
    if unknown:
        raise TypeError(f"unknown SandboxOptions field(s): {sorted(unknown)}")

    kwargs = dict(options)
    if kwargs.get("secrets"):
        kwargs["secrets"] = {k: _coerce_secret(v) for k, v in kwargs["secrets"].items()}
    if kwargs.get("bind_mounts"):
        kwargs["bind_mounts"] = [_coerce_bind_mount(m) for m in kwargs["bind_mounts"]]
    return SandboxOptions(**kwargs)
