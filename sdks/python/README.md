# zerobox (Python)

Python SDK for [zerobox](https://github.com/afshinm/zerobox) — sandbox any command with file, network, and credential controls.

```bash
pip install zerobox
```

Installing the wheel drops the `zerobox` CLI into your environment's `bin/` and exposes a Python SDK that mirrors the TypeScript one.

## Quick start

```python
from zerobox import Sandbox

sandbox = Sandbox.create({"allow_write": ["/tmp"]})
print(sandbox.sh("echo hello").text())
```

Run inline Python in the sandbox:

```python
data = sandbox.py("import json; print(json.dumps({'sum': 1 + 2}))").json()
```

Explicit command + args:

```python
sandbox.exec("python3", ["-c", "print('hi')"]).text()
```

Raw output (doesn't raise on non-zero exit):

```python
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

```python
sandbox = Sandbox.create({
    "secrets": {
        "OPENAI_API_KEY": {
            "value": "sk-proj-123",
            "hosts": ["api.openai.com"],
        },
    },
})

sandbox.sh("curl -H \"Authorization: Bearer $OPENAI_API_KEY\" https://api.openai.com/v1/models").text()
```

The sandboxed process sees only a placeholder; the real value is substituted at the network proxy for the listed hosts.

## Timeouts

```python
sandbox.sh("sleep 60").text(timeout=1.0)  # raises subprocess.TimeoutExpired
```

## Options

Same shape as the TypeScript SDK, in `snake_case`:

| Field | Type | Description |
| --- | --- | --- |
| `profile` | `str \| list[str]` | Named profile(s). A list merges left-to-right. Default: `"workspace"`. |
| `allow_read` / `deny_read` | `list[str]` | Readable / blocked paths. |
| `allow_write` / `deny_write` | `list[str]` | Writable / blocked paths. |
| `allow_net` | `bool \| list[str]` | `True` = all; list restricts to domains. |
| `deny_net` | `list[str]` | Blocked domains. |
| `allow_all` | `bool` | Full filesystem + network access. |
| `no_sandbox` | `bool` | Disable the sandbox entirely. |
| `strict_sandbox` | `bool` | Fail instead of falling back to weaker isolation. |
| `cwd` | `str` | Working directory. |
| `env` | `dict[str, str]` | Explicit env vars. |
| `allow_env` | `bool \| list[str]` | Inherit parent env vars. |
| `deny_env` | `list[str]` | Blocked env vars. |
| `snapshot` | `bool` | Record filesystem changes. |
| `restore` | `bool` | Record and roll back after exit. |
| `snapshot_paths` / `snapshot_exclude` | `list[str]` | Tracked paths / excluded patterns. |
| `secrets` | `dict[str, SecretConfig]` | Secrets with per-host scopes. |
| `debug` | `bool` | Print sandbox config to stderr. |

See the [main README](https://github.com/afshinm/zerobox) for the full CLI reference.

## License

Apache-2.0
