"""Port of packages/zerobox/src/errors.test.ts."""

from __future__ import annotations

from zerobox import CommandOutput, SandboxCommandError


def test_stderr_wins_over_stdout():
    err = SandboxCommandError(CommandOutput(code=1, stdout="out", stderr="err message"))
    assert str(err) == "err message"


def test_falls_back_to_stdout_when_stderr_empty():
    err = SandboxCommandError(CommandOutput(code=1, stdout="out message", stderr=""))
    assert str(err) == "out message"


def test_falls_back_to_generic_when_both_empty():
    err = SandboxCommandError(CommandOutput(code=42, stdout="", stderr=""))
    assert str(err) == "command exited with code 42"


def test_whitespace_only_is_treated_as_empty():
    err = SandboxCommandError(CommandOutput(code=1, stdout="  \n\n", stderr="   "))
    assert str(err) == "command exited with code 1"


def test_preserves_fields():
    err = SandboxCommandError(CommandOutput(code=2, stdout="o", stderr="e"))
    assert err.code == 2
    assert err.stdout == "o"
    assert err.stderr == "e"


def test_is_exception():
    err = SandboxCommandError(CommandOutput(code=1, stdout="", stderr=""))
    assert isinstance(err, Exception)
