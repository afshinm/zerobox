from __future__ import annotations

from typing import Any, Union

from .options import SandboxOptions, normalize_options


def _add_csv(flags: list[str], name: str, values: Union[list[str], None]) -> None:
    if values:
        flags.append(f"--{name}={','.join(values)}")


def _add_allow(flags: list[str], name: str, value: Union[bool, list[str], None]) -> None:
    if value is True:
        flags.append(f"--{name}")
    elif isinstance(value, list):
        _add_csv(flags, name, value)


def _profile_list(profile: Union[str, list[str], None]) -> list[str]:
    if isinstance(profile, list) and profile:
        return list(profile)
    if isinstance(profile, str) and profile:
        return [profile]
    return ["workspace"]


def build_flags(options: Union[SandboxOptions, dict[str, Any], None]) -> list[str]:
    """Convert SandboxOptions → CLI flags. Byte-compatible with TS `buildFlags`."""
    o = normalize_options(options)
    flags: list[str] = []
    secret_hosts: list[str] = []

    if o.secrets:
        for key, cfg in o.secrets.items():
            flags.extend(["--secret", f"{key}={cfg.value}"])
            if cfg.hosts:
                flags.extend(["--secret-host", f"{key}={','.join(cfg.hosts)}"])
                secret_hosts.extend(cfg.hosts)

    if o.strict_sandbox:
        flags.append("--strict-sandbox")
    if o.debug:
        flags.append("--debug")

    if o.allow_all:
        flags.append("--allow-all")
    elif o.no_sandbox:
        flags.append("--no-sandbox")
    else:
        for p in _profile_list(o.profile):
            flags.extend(["--profile", p])

        _add_csv(flags, "allow-read", o.allow_read)
        _add_csv(flags, "deny-read", o.deny_read)
        _add_csv(flags, "allow-write", o.allow_write)
        _add_csv(flags, "deny-write", o.deny_write)

        # Secret hosts implicitly allow network for their domains. When
        # allow_net is already a list, append the hosts. When True, leave
        # alone. When unset, let the CLI gate network via secret-host.
        allow_net = o.allow_net
        if secret_hosts and isinstance(allow_net, list):
            allow_net = [*allow_net, *secret_hosts]

        _add_allow(flags, "allow-net", allow_net)
        _add_csv(flags, "deny-net", o.deny_net)

    _add_allow(flags, "allow-env", o.allow_env)
    _add_csv(flags, "deny-env", o.deny_env)
    if o.env:
        for key, value in o.env.items():
            flags.extend(["--env", f"{key}={value}"])

    if o.cwd:
        flags.extend(["-C", o.cwd])

    if o.restore:
        flags.append("--restore")
    elif o.snapshot:
        flags.append("--snapshot")
    _add_csv(flags, "snapshot-path", o.snapshot_paths)
    _add_csv(flags, "snapshot-exclude", o.snapshot_exclude)

    return flags
