# Zerobox Python SDK

<p>
  <a href="https://pypi.org/project/zerobox/" target="_blank">
    <img src="https://img.shields.io/pypi/v/zerobox?style=for-the-badge&labelColor=000000&label=PyPI" alt="Zerobox PyPI version" />
  </a>
  <a href="https://github.com/afshinm/zerobox/blob/main/LICENSE" target="_blank">
    <img src="https://img.shields.io/github/license/afshinm/zerobox?style=for-the-badge&labelColor=000000" alt="Zerobox license" />
  </a>
</p>

Python SDK for [zerobox](https://github.com/afshinm/zerobox). Sandbox any command with file, network, and credential controls.

```bash
pip install zerobox
```

Installing the wheel drops the `zerobox` CLI into your environment's `bin/` and exposes a Python SDK.

> For CLI usage, secrets concepts, the full flag reference, performance numbers, and platform support see the [main README](https://github.com/afshinm/zerobox).

## Quick start

```python
from zerobox import Sandbox

sandbox = Sandbox.create({"allow_write": ["/tmp"]})
print(sandbox.sh("echo hello").text())
```

## Commands

Three ways to run a command. Each returns a `ShellCommand` you terminate with `.text()`, `.json()`, or `.output()`.

### Shell

```python
name = "world"
sandbox.sh(f"echo hello {name}").text()
```

### Inline Python

```python
data = sandbox.py("import json; print(json.dumps({'sum': 1 + 2}))").json()
```

### Explicit command + args

```python
sandbox.exec("python3", ["-c", "print('hi')"]).text()
```

## Results

| Method | On success | On non-zero exit |
| --- | --- | --- |
| `.text()` | Returns stdout as a string | Raises `SandboxCommandError` |
| `.json()` | Parses stdout as JSON | Raises `SandboxCommandError` |
| `.output()` | Returns `CommandOutput(code, stdout, stderr)` | Returns the same shape, never raises |

```python
data = sandbox.sh("cat data.json").json()
result = sandbox.sh("exit 42").output()
# CommandOutput(code=42, stdout='', stderr='')
```

## Error handling

Non-zero exit raises `SandboxCommandError`:

```python
from zerobox import Sandbox, SandboxCommandError

sandbox = Sandbox.create()
try:
    sandbox.sh("exit 1").text()
except SandboxCommandError as e:
    print(e.code, e.stderr)
```

## Secrets

Pass API keys that the sandboxed process never sees. The proxy substitutes the real value only for approved hosts.

```python
import os
from zerobox import Sandbox

sandbox = Sandbox.create({
    "secrets": {
        "OPENAI_API_KEY": {
            "value": os.environ["OPENAI_API_KEY"],
            "hosts": ["api.openai.com"],
        },
        "GITHUB_TOKEN": {
            "value": os.environ["GITHUB_TOKEN"],
            "hosts": ["api.github.com"],
        },
    },
})

sandbox.sh('curl -H "Authorization: Bearer $OPENAI_API_KEY" https://api.openai.com/v1/models').text()
```

See the [main README](https://github.com/afshinm/zerobox#secrets) for how placeholder substitution works.

## Snapshots

Record filesystem changes and roll them back automatically:

```python
sandbox = Sandbox.create({
    "allow_write": ["."],
    "restore": True,
})
sandbox.sh("npm install").text()
```

Record without rolling back:

```python
sandbox = Sandbox.create({
    "allow_write": ["."],
    "snapshot": True,
    "snapshot_exclude": ["node_modules"],
})
sandbox.sh("npm install").text()
```

## Cancellation

Pass a `timeout` (seconds) to any terminator:

```python
import subprocess
try:
    sandbox.sh("sleep 60").text(timeout=1.0)
except subprocess.TimeoutExpired:
    print("cancelled")
```

## Environment variables

```python
sandbox = Sandbox.create({
    "env": {"NODE_ENV": "production"},
    "allow_env": ["PATH", "HOME"],
    "deny_env": ["AWS_SECRET_ACCESS_KEY"],
})
```

See the [main README](https://github.com/afshinm/zerobox#environment-variables) for what's inherited by default and the CLI equivalents.

## Options

`Sandbox.create(options)` accepts a `SandboxOptions` dataclass or a plain dict. All fields are optional.

| Field | Type | Description |
| --- | --- | --- |
| `profile` | `str \| list[str]` | Named profile(s). A list merges left-to-right. Default `"workspace"`. |
| `allow_read` / `deny_read` | `list[str]` | Readable / blocked paths. |
| `allow_write` / `deny_write` | `list[str]` | Writable / blocked paths. |
| `allow_net` | `bool \| list[str]` | `True` allows all. A list restricts to those domains. |
| `deny_net` | `list[str]` | Blocked domains. |
| `allow_all` | `bool` | Full filesystem + network access. |
| `no_sandbox` | `bool` | Disable the sandbox entirely. |
| `strict_sandbox` | `bool` | Fail instead of falling back to weaker isolation. |
| `cwd` | `str` | Working directory. |
| `env` | `dict[str, str]` | Explicit env vars. |
| `allow_env` | `bool \| list[str]` | Inherit parent env vars. |
| `deny_env` | `list[str]` | Blocked env vars. |
| `snapshot` | `bool` | Record filesystem changes. |
| `restore` | `bool` | Record and roll back after exit. Implies `snapshot`. |
| `snapshot_paths` / `snapshot_exclude` | `list[str]` | Tracked paths / excluded patterns. |
| `secrets` | `dict[str, SecretConfig]` | Secrets with per-host scopes. |
| `debug` | `bool` | Print sandbox config to stderr. |

Unknown dict keys (e.g. accidental `allowWrite` instead of `allow_write`) raise `TypeError` at construction time.

## Caveats

`Sandbox.py(code)` runs whichever `python3` is on PATH inside the sandbox. If your active interpreter lives outside the sandbox's readable roots (for example uv-managed Pythons under `~/.local/share/uv/`), fall back to:

```python
import sys
sandbox = Sandbox.create({"allow_read": [sys.prefix]})
sandbox.exec(sys.executable, ["-c", "print('hi')"]).text()
```

## Other SDKs

- [TypeScript SDK](https://github.com/afshinm/zerobox/tree/main/packages/zerobox) (npm: `zerobox`)
- [Rust SDK](https://github.com/afshinm/zerobox/tree/main/crates/zerobox) (crates.io: `zerobox`)

## License

Apache-2.0
