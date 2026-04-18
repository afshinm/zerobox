"""Port of packages/zerobox/src/platforms.test.ts.

The TS tests use JS arch names ("x64", "arm64"). We use Python conventions
("x86_64", "arm64") so package names differ: "zerobox-linux-x86_64" instead of
"@zerobox/cli-linux-x64". Semantics are identical.
"""

from __future__ import annotations

from zerobox.platforms import PlatformEnv, detect_musl, platform_package


def make_env(**overrides) -> PlatformEnv:
    defaults = {
        "platform": "linux",
        "arch": "x86_64",
        "linker_exists": lambda _: False,
        "libc_version": lambda: None,
        "ldd_output": lambda: None,
        "os_release": lambda: None,
    }
    defaults.update(overrides)
    return PlatformEnv(**defaults)


# ── detect_musl ──


def test_non_linux_returns_false():
    assert detect_musl(make_env(platform="darwin")) is False
    assert detect_musl(make_env(platform="win32")) is False


def test_detects_musl_via_linker_x86_64():
    env = make_env(arch="x86_64", linker_exists=lambda p: p == "/lib/ld-musl-x86_64.so.1")
    assert detect_musl(env) is True


def test_detects_musl_via_linker_arm64():
    env = make_env(arch="arm64", linker_exists=lambda p: p == "/lib/ld-musl-aarch64.so.1")
    assert detect_musl(env) is True


def test_skips_linker_for_unknown_arch():
    env = make_env(arch="s390x", linker_exists=lambda _: True, ldd_output=lambda: "musl libc")
    assert detect_musl(env) is True


def test_glibc_version_reported_returns_false():
    assert detect_musl(make_env(libc_version=lambda: "2.39")) is False


def test_continues_when_libc_version_missing():
    env = make_env(ldd_output=lambda: "musl libc (x86_64)\nVersion 1.2.4")
    assert detect_musl(env) is True


def test_detects_musl_from_ldd_output():
    env = make_env(ldd_output=lambda: "musl libc (x86_64)\nVersion 1.2.4")
    assert detect_musl(env) is True


def test_detects_glibc_from_ldd_output():
    env = make_env(ldd_output=lambda: "ldd (GNU libc) 2.39")
    assert detect_musl(env) is False


def test_continues_when_ldd_unknown():
    env = make_env(
        ldd_output=lambda: "some unknown output",
        os_release=lambda: 'NAME="Alpine Linux"\nID=alpine',
    )
    assert detect_musl(env) is True


def test_detects_musl_via_alpine_os_release():
    env = make_env(os_release=lambda: 'NAME="Alpine Linux"\nID=alpine\nVERSION_ID=3.19.0')
    assert detect_musl(env) is True


def test_non_alpine_os_release_returns_false():
    env = make_env(os_release=lambda: 'NAME="Ubuntu"\nID=ubuntu\nVERSION_ID="24.04"')
    assert detect_musl(env) is False


def test_nothing_matches_returns_false():
    assert detect_musl(make_env()) is False


def test_linker_takes_priority_over_ldd():
    env = make_env(
        arch="x86_64",
        linker_exists=lambda p: p == "/lib/ld-musl-x86_64.so.1",
        ldd_output=lambda: "ldd (GNU libc) 2.39",
    )
    assert detect_musl(env) is True


def test_glibc_version_takes_priority_over_ldd_musl():
    env = make_env(libc_version=lambda: "2.39", ldd_output=lambda: "musl libc")
    assert detect_musl(env) is False


# ── platform_package ──


def test_darwin_arm64():
    assert platform_package(make_env(platform="darwin", arch="arm64")) == "zerobox-darwin-arm64"


def test_darwin_x86_64():
    assert platform_package(make_env(platform="darwin", arch="x86_64")) == "zerobox-darwin-x86_64"


def test_glibc_linux_x86_64():
    env = make_env(ldd_output=lambda: "ldd (GNU libc) 2.39")
    assert platform_package(env) == "zerobox-linux-x86_64"


def test_musl_linux_x86_64():
    env = make_env(arch="x86_64", linker_exists=lambda p: p == "/lib/ld-musl-x86_64.so.1")
    assert platform_package(env) == "zerobox-linux-x86_64-musl"


def test_musl_linux_arm64():
    env = make_env(arch="arm64", linker_exists=lambda p: p == "/lib/ld-musl-aarch64.so.1")
    assert platform_package(env) == "zerobox-linux-arm64-musl"


def test_unsupported_platform_returns_none():
    assert platform_package(make_env(platform="freebsd")) is None


def test_unsupported_arch_returns_none():
    assert platform_package(make_env(platform="darwin", arch="s390x")) is None


def test_falls_back_to_glibc_when_musl_unavailable_for_arch():
    env = make_env(arch="s390x", linker_exists=lambda _: True)
    assert platform_package(env) is None
