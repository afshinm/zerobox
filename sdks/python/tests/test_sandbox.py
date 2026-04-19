"""End-to-end tests. Port of packages/zerobox/src/sandbox.test.ts.

Skipped unless ZEROBOX_BIN points at a working zerobox binary, mirroring the TS
SDK. On Linux, this also requires unprivileged user namespaces (the CI workflow
enables them).
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import time
from pathlib import Path

import pytest

from zerobox import Sandbox, SandboxCommandError

pytestmark = pytest.mark.skipif(not os.environ.get("ZEROBOX_BIN"), reason="ZEROBOX_BIN not set")


# uv-managed Python lives outside the sandbox's default readable roots on macOS,
# so `python3 -c` can't load its dylib inside a sandbox. Tests that exercise a
# Python interpreter inside the sandbox use the OS-provided one.
SYS_PYTHON = "/usr/bin/python3" if Path("/usr/bin/python3").exists() else "python3"


def _rm(path: str) -> None:
    p = Path(path)
    if p.is_file() or p.is_symlink():
        p.unlink(missing_ok=True)
    elif p.exists():
        shutil.rmtree(path, ignore_errors=True)


@pytest.fixture
def cleanup_path():
    to_remove: list[str] = []

    def register(path: str) -> str:
        _rm(path)
        to_remove.append(path)
        return path

    yield register

    for path in to_remove:
        _rm(path)


def test_sh_text_returns_stdout():
    assert Sandbox.create().sh("echo hello").text().strip() == "hello"


def test_sh_text_raises_on_non_zero():
    with pytest.raises(SandboxCommandError) as exc:
        Sandbox.create().sh("exit 42").text()
    assert exc.value.code == 42


def test_sh_json_parses_stdout():
    data = Sandbox.create().sh('echo \'{"key":"value"}\'').json()
    assert data["key"] == "value"


def test_sh_output_never_raises():
    result = Sandbox.create().sh("exit 42").output()
    assert result.code == 42


def test_sh_output_captures_stdout_and_stderr():
    result = Sandbox.create().sh("echo out && echo err >&2").output()
    assert result.code == 0
    assert result.stdout.strip() == "out"
    assert result.stderr.strip() == "err"


def test_exec_python3_runs_inline_code():
    assert Sandbox.create().exec(SYS_PYTHON, ["-c", "print(1 + 1)"]).text().strip() == "2"


def test_exec_python3_json_parses_output():
    data = (
        Sandbox.create()
        .exec(
            SYS_PYTHON,
            ["-c", "import json; print(json.dumps({'sum': 1 + 2}))"],
        )
        .json()
    )
    assert data["sum"] == 3


@pytest.fixture(scope="module")
def _sandbox_py_available():
    # Probe whether `python3` on PATH can run inside the sandbox.
    probe = Sandbox.create().py("pass").output()
    if probe.code != 0:
        pytest.skip(f"python3 unavailable inside sandbox: {probe.stderr.strip()[:200]}")


def test_sandbox_py_runs_inline_code(_sandbox_py_available):
    assert Sandbox.create().py("print(1 + 1)").text().strip() == "2"


def test_sandbox_py_json_parses_output(_sandbox_py_available):
    data = Sandbox.create().py("import json; print(json.dumps({'sum': 1 + 2}))").json()
    assert data["sum"] == 3


def test_sandbox_py_raises_on_non_zero(_sandbox_py_available):
    with pytest.raises(SandboxCommandError) as exc:
        Sandbox.create().py("import sys; sys.exit(7)").text()
    assert exc.value.code == 7


def test_exec_runs_with_args():
    assert Sandbox.create().exec("echo", ["hello"]).text().strip() == "hello"


def test_workspace_can_read_cwd():
    result = Sandbox.create().sh("ls .").output()
    assert result.code == 0
    assert result.stdout


def test_workspace_can_write_to_cwd(cleanup_path):
    name = cleanup_path(f"zerobox-sdk-cwd-{int(time.time() * 1000)}")
    Sandbox.create().sh(f"echo ok > {name}").output()
    assert Path(name).exists()
    assert Path(name).read_text().strip() == "ok"


def test_blocks_writes_outside_allowed_paths():
    home = os.environ.get("HOME", "/tmp")
    target = f"{home}/zerobox-sdk-wb-{int(time.time() * 1000)}"
    result = Sandbox.create().sh(f"echo x > {target} 2>&1 || echo BLOCKED").output()
    assert re.search(
        r"BLOCKED|Read-only|Permission denied|Operation not permitted|No such file",
        result.stdout + result.stderr,
        re.IGNORECASE,
    )


def test_allow_write_enables_tmp(cleanup_path):
    target = cleanup_path("/tmp/zerobox-sdk-aw")
    sandbox = Sandbox.create({"allow_write": ["/tmp"]})
    sandbox.sh(f"echo ok > {target}").output()
    assert Path(target).exists()
    assert Path(target).read_text().strip() == "ok"


def test_deny_write_overrides_allow_write(cleanup_path, tmp_path):
    workdir = cleanup_path(str(tmp_path / "work"))
    Path(workdir).mkdir(parents=True, exist_ok=True)
    Path(workdir, ".git").mkdir()

    sandbox = Sandbox.create(
        {
            "cwd": workdir,
            "allow_read": [workdir],
            "allow_write": [workdir],
            "deny_write": [f"{workdir}/.git"],
        }
    )
    script = (
        "import os, sys\n"
        "out = []\n"
        "try:\n"
        f"    open('{workdir}/ok.txt', 'w').write('x'); out.append('file:ok')\n"
        "except OSError as e:\n"
        "    out.append(f'file:blocked:{e.errno}')\n"
        "try:\n"
        f"    open('{workdir}/.git/evil', 'w').write('x'); out.append('git:ok')\n"
        "except OSError as e:\n"
        "    out.append(f'git:blocked:{e.errno}')\n"
        "print(','.join(out))\n"
    )
    output = sandbox.exec(SYS_PYTHON, ["-c", script]).text()
    assert not Path(workdir, ".git", "evil").exists()
    assert "git:blocked" in output
    assert "git:ok" not in output


def test_network_blocked_by_default():
    result = (
        Sandbox.create()
        .exec(
            "curl",
            [
                "-s",
                "--max-time",
                "5",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "https://example.com",
            ],
        )
        .output()
    )
    assert result.stdout.strip() != "200"


def test_allow_net_true_enables_all():
    sandbox = Sandbox.create({"allow_net": True})
    result = sandbox.exec(
        "curl",
        ["-s", "-o", "/dev/null", "-w", "%{http_code}", "https://example.com"],
    ).text()
    assert result.strip() == "200"


def test_allow_net_list_restricts():
    sandbox = Sandbox.create({"allow_net": ["example.com"]})
    result = sandbox.exec(
        "curl",
        ["-s", "-o", "/dev/null", "-w", "%{http_code}", "https://example.com"],
    ).text()
    assert result.strip() == "200"


def test_allow_net_blocks_unlisted():
    sandbox = Sandbox.create({"allow_net": ["example.com"]})
    result = sandbox.exec(
        "curl",
        [
            "-s",
            "--max-time",
            "5",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "https://google.com",
        ],
    ).output()
    assert result.stdout.strip() != "200"


def test_timeout_kills_long_running_child():
    sandbox = Sandbox.create()
    with pytest.raises(subprocess.TimeoutExpired):
        sandbox.sh("sleep 10").text(timeout=0.5)


def test_allow_all_enables_everything(cleanup_path):
    target = cleanup_path("/tmp/zerobox-sdk-aa")
    Sandbox.create({"allow_all": True}).sh(f"echo ok > {target}").output()
    assert Path(target).exists()
    assert Path(target).read_text().strip() == "ok"


def test_default_env_excludes_custom_parent_vars():
    os.environ["ZEROBOX_TEST_CUSTOM"] = "leaked"
    try:
        out = Sandbox.create().sh("echo $ZEROBOX_TEST_CUSTOM").text().strip()
        assert out == ""
    finally:
        os.environ.pop("ZEROBOX_TEST_CUSTOM", None)


def test_default_env_includes_path():
    assert Sandbox.create().sh("echo $PATH").text().strip()


def test_env_sets_explicit_var():
    out = Sandbox.create({"env": {"MY_VAR": "hello"}}).sh("echo $MY_VAR").text()
    assert out.strip() == "hello"


def test_env_multiple():
    out = Sandbox.create({"env": {"A": "1", "B": "2"}}).sh("echo $A $B").text()
    assert out.strip() == "1 2"


def test_allow_env_true_inherits_all():
    sandbox = Sandbox.create({"allow_env": True})
    out = sandbox.sh("env").text()
    assert len(out.strip().splitlines()) > 10


def test_allow_env_specific_keys():
    sandbox = Sandbox.create({"allow_env": ["PATH"]})
    lines = sandbox.sh("env").text().strip().splitlines()
    assert any(line.startswith("PATH=") for line in lines)
    assert not any(line.startswith("HOME=") for line in lines)


def test_deny_env_removes_vars():
    sandbox = Sandbox.create({"allow_env": True, "deny_env": ["HOME"]})
    out = sandbox.sh('echo "HOME=$HOME"').text()
    assert out.strip() == "HOME="


def test_deny_env_does_not_block_explicit():
    sandbox = Sandbox.create({"deny_env": ["FOO"], "env": {"FOO": "override"}})
    assert sandbox.sh("echo $FOO").text().strip() == "override"


def test_env_value_with_equals_sign():
    sandbox = Sandbox.create({"env": {"DATA": "a=b=c"}})
    assert sandbox.sh("echo $DATA").text().strip() == "a=b=c"


def test_secret_env_has_placeholder_not_real_value():
    sandbox = Sandbox.create(
        {"secrets": {"API_KEY": {"value": "sk-test-123", "hosts": ["example.com"]}}}
    )
    out = sandbox.sh("echo $API_KEY").text().strip()
    assert re.match(r"^ZEROBOX_SECRET_[0-9a-f]{64}$", out)
    assert out != "sk-test-123"


def test_secret_header_substituted_for_matching_host():
    sandbox = Sandbox.create(
        {"secrets": {"MY_SECRET": {"value": "real-value", "hosts": ["httpbin.org"]}}}
    )
    data = sandbox.sh('curl -sk -H "X-Test: $MY_SECRET" https://httpbin.org/headers').json()
    assert data["headers"]["X-Test"] == "real-value"


def test_secret_not_substituted_for_wrong_host():
    sandbox = Sandbox.create(
        {
            "allow_net": True,
            "secrets": {"MY_SECRET": {"value": "real-value", "hosts": ["other.com"]}},
        }
    )
    data = sandbox.sh('curl -sk -H "X-Test: $MY_SECRET" https://httpbin.org/headers').json()
    assert data["headers"]["X-Test"].startswith("ZEROBOX_SECRET_")


def test_multiple_secrets_per_host():
    sandbox = Sandbox.create(
        {
            "allow_net": True,
            "secrets": {
                "SECRET_A": {"value": "value-a", "hosts": ["httpbin.org"]},
                "SECRET_B": {"value": "value-b", "hosts": ["other.com"]},
            },
        }
    )
    data = sandbox.sh(
        'curl -sk -H "X-A: $SECRET_A" -H "X-B: $SECRET_B" https://httpbin.org/headers'
    ).json()
    assert data["headers"]["X-A"] == "value-a"
    assert data["headers"]["X-B"].startswith("ZEROBOX_SECRET_")


def test_env_and_secrets_together():
    sandbox = Sandbox.create(
        {
            "env": {"MY_VAR": "env-val"},
            "secrets": {"MY_SECRET": {"value": "secret-val", "hosts": ["httpbin.org"]}},
        }
    )
    assert sandbox.sh("echo $MY_VAR").text().strip() == "env-val"
    assert sandbox.sh("echo $MY_SECRET").text().strip().startswith("ZEROBOX_SECRET_")


def test_multi_profile_merges():
    sandbox = Sandbox.create({"profile": ["workspace"]})
    assert sandbox.sh("ls .").output().code == 0


def test_context_manager():
    with Sandbox.create() as sandbox:
        assert sandbox.sh("echo hi").text().strip() == "hi"
