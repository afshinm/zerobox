"""Port of packages/zerobox/src/flags.test.ts. Must stay in parity."""

from __future__ import annotations

from zerobox import build_flags


def test_defaults_to_workspace_profile():
    assert build_flags({}) == ["--profile", "workspace"]


def test_custom_profile():
    assert build_flags({"profile": "claude"}) == ["--profile", "claude"]


def test_profile_list_emits_one_flag_per_entry():
    assert build_flags({"profile": ["workspace", "git-config"]}) == [
        "--profile",
        "workspace",
        "--profile",
        "git-config",
    ]


def test_single_element_list_matches_string():
    assert build_flags({"profile": ["claude"]}) == build_flags({"profile": "claude"})


def test_empty_profile_list_falls_back_to_workspace():
    assert build_flags({"profile": []}) == ["--profile", "workspace"]


def test_allow_all_strips_fs_profile_flags():
    assert build_flags({"allow_all": True, "allow_write": ["/tmp"]}) == ["--allow-all"]


def test_no_sandbox_strips_profile_flags():
    assert build_flags({"no_sandbox": True, "allow_write": ["/tmp"]}) == ["--no-sandbox"]


def test_strict_sandbox_coexists_with_profile():
    flags = build_flags({"strict_sandbox": True, "allow_write": ["/tmp"]})
    assert "--strict-sandbox" in flags
    assert "--allow-write=/tmp" in flags


def test_allow_read():
    flags = build_flags({"allow_read": ["/tmp", "/data"]})
    assert "--allow-read=/tmp,/data" in flags


def test_deny_read():
    assert "--deny-read=/secret" in build_flags({"deny_read": ["/secret"]})


def test_allow_write():
    assert "--allow-write=/tmp" in build_flags({"allow_write": ["/tmp"]})


def test_deny_write():
    assert "--deny-write=.git" in build_flags({"deny_write": [".git"]})


def test_allow_net_true():
    assert "--allow-net" in build_flags({"allow_net": True})


def test_allow_net_domains():
    flags = build_flags({"allow_net": ["example.com", "api.example.com"]})
    assert "--allow-net=example.com,api.example.com" in flags


def test_allow_net_false_not_emitted():
    assert "--allow-net" not in build_flags({"allow_net": False})


def test_allow_net_empty_list_not_emitted():
    flags = build_flags({"allow_net": []})
    assert not [f for f in flags if f.startswith("--allow-net")]


def test_deny_net():
    assert "--deny-net=evil.com" in build_flags({"deny_net": ["evil.com"]})


def test_cwd_emits_dash_C():
    flags = build_flags({"cwd": "/workspace"})
    assert "-C" in flags and "/workspace" in flags


def test_combines_multiple_flags():
    flags = build_flags(
        {
            "allow_read": ["/tmp"],
            "deny_read": ["/tmp/secret"],
            "allow_write": ["/tmp"],
            "deny_write": ["/tmp/.git"],
            "allow_net": ["example.com"],
            "deny_net": ["evil.com"],
            "cwd": "/workspace",
        }
    )
    for expected in [
        "--profile",
        "--allow-read=/tmp",
        "--deny-read=/tmp/secret",
        "--allow-write=/tmp",
        "--deny-write=/tmp/.git",
        "--allow-net=example.com",
        "--deny-net=evil.com",
        "-C",
        "/workspace",
    ]:
        assert expected in flags


def test_skips_empty_arrays():
    flags = build_flags({"allow_read": [], "deny_read": [], "allow_write": [], "deny_write": []})
    assert flags == ["--profile", "workspace"]


def test_env_single():
    flags = build_flags({"env": {"FOO": "bar"}})
    assert "--env" in flags and "FOO=bar" in flags


def test_env_multiple():
    flags = build_flags({"env": {"A": "1", "B": "2"}})
    assert "--env" in flags and "A=1" in flags and "B=2" in flags


def test_allow_env_true():
    assert "--allow-env" in build_flags({"allow_env": True})


def test_allow_env_keys():
    assert "--allow-env=PATH,HOME" in build_flags({"allow_env": ["PATH", "HOME"]})


def test_deny_env():
    assert "--deny-env=SECRET" in build_flags({"deny_env": ["SECRET"]})


def test_secrets_with_hosts():
    flags = build_flags({"secrets": {"API_KEY": {"value": "sk-123", "hosts": ["api.example.com"]}}})
    assert "--secret" in flags
    assert "API_KEY=sk-123" in flags
    assert "--secret-host" in flags
    assert "API_KEY=api.example.com" in flags


def test_secret_without_hosts():
    flags = build_flags({"secrets": {"TOKEN": {"value": "abc", "hosts": []}}})
    assert "--secret" in flags
    assert "TOKEN=abc" in flags
    assert "--secret-host" not in flags


def test_secret_hosts_merge_into_allow_net_list():
    flags = build_flags(
        {
            "allow_net": ["other.com"],
            "secrets": {"KEY": {"value": "v", "hosts": ["api.com"]}},
        }
    )
    assert "--allow-net=other.com,api.com" in flags


def test_secrets_do_not_duplicate_allow_net_true():
    flags = build_flags(
        {
            "allow_net": True,
            "secrets": {"KEY": {"value": "v", "hosts": ["api.com"]}},
        }
    )
    assert "--allow-net" in flags
    assert len([f for f in flags if f.startswith("--allow-net")]) == 1


def test_multiple_secrets():
    flags = build_flags(
        {
            "secrets": {
                "A": {"value": "v1", "hosts": ["h1.com"]},
                "B": {"value": "v2", "hosts": ["h2.com"]},
            }
        }
    )
    assert len([f for f in flags if f == "--secret"]) == 2
    assert "A=v1" in flags and "B=v2" in flags


def test_env_flags_coexist_with_allow_all():
    flags = build_flags({"allow_all": True, "env": {"FOO": "bar"}})
    assert "--allow-all" in flags
    assert "--env" in flags and "FOO=bar" in flags


def test_secrets_coexist_with_allow_all():
    flags = build_flags(
        {
            "allow_all": True,
            "secrets": {"KEY": {"value": "v", "hosts": ["h.com"]}},
        }
    )
    assert "--allow-all" in flags
    assert "--secret" in flags and "KEY=v" in flags


def test_deny_env_with_secrets():
    flags = build_flags(
        {
            "deny_env": ["HOME"],
            "secrets": {"KEY": {"value": "v", "hosts": ["h.com"]}},
        }
    )
    assert "--deny-env=HOME" in flags
    assert "--secret" in flags and "KEY=v" in flags


def test_no_sandbox_with_secrets():
    flags = build_flags(
        {
            "no_sandbox": True,
            "secrets": {"KEY": {"value": "v", "hosts": ["h.com"]}},
        }
    )
    assert "--no-sandbox" in flags
    assert "--secret" in flags and "KEY=v" in flags


def test_secrets_alone_do_not_emit_allow_net():
    flags = build_flags({"secrets": {"KEY": {"value": "v", "hosts": ["h.com"]}}})
    assert "--secret" in flags and "--secret-host" in flags
    assert not [f for f in flags if f.startswith("--allow-net")]


def test_allow_env_false_not_emitted():
    assert "--allow-env" not in build_flags({"allow_env": False})


def test_allow_env_empty_list_not_emitted():
    assert "--allow-env" not in build_flags({"allow_env": []})


def test_debug_flag():
    assert "--debug" in build_flags({"debug": True})


def test_snapshot_flag():
    assert "--snapshot" in build_flags({"snapshot": True})


def test_restore_flag_overrides_snapshot():
    flags = build_flags({"snapshot": True, "restore": True})
    assert "--restore" in flags
    assert "--snapshot" not in flags


def test_snapshot_paths_and_exclude():
    flags = build_flags(
        {
            "snapshot": True,
            "snapshot_paths": ["/a", "/b"],
            "snapshot_exclude": ["node_modules"],
        }
    )
    assert "--snapshot-path=/a,/b" in flags
    assert "--snapshot-exclude=node_modules" in flags


def test_accepts_dataclass_and_dict_identically():
    from zerobox import SandboxOptions

    dc = SandboxOptions(allow_write=["/tmp"], allow_net=["example.com"])
    dt = {"allow_write": ["/tmp"], "allow_net": ["example.com"]}
    assert build_flags(dc) == build_flags(dt)


# bind mounts


def test_bind_mount_single_entry():
    flags = build_flags({"bind_mounts": [{"host": "/tmp/proj-abc", "sandbox": "/tmp"}]})
    assert "--bind-mount" in flags
    assert "/tmp/proj-abc:/tmp" in flags


def test_bind_mount_read_only_appends_ro():
    flags = build_flags(
        {
            "bind_mounts": [
                {"host": "/var/cache/pkg", "sandbox": "/var/cache/pkg", "read_only": True}
            ]
        }
    )
    assert "--bind-mount" in flags
    assert "/var/cache/pkg:/var/cache/pkg:ro" in flags


def test_bind_mount_preserves_windows_drive_letter_paths():
    flags = build_flags(
        {
            "bind_mounts": [
                {
                    "host": r"C:\host\a",
                    "sandbox": r"D:\sandbox\a",
                    "read_only": True,
                }
            ]
        }
    )
    assert r"C:\host\a:D:\sandbox\a:ro" in flags


def test_bind_mount_preserves_argv_order():
    flags = build_flags(
        {
            "bind_mounts": [
                {"host": "/host/a", "sandbox": "/a"},
                {"host": "/host/b", "sandbox": "/a/b", "read_only": True},
                {"host": "/host/c", "sandbox": "/c"},
            ]
        }
    )
    specs = [flags[i + 1] for i, f in enumerate(flags) if f == "--bind-mount"]
    assert specs == ["/host/a:/a", "/host/b:/a/b:ro", "/host/c:/c"]


def test_bind_mount_missing_or_empty_omits_flag():
    assert "--bind-mount" not in build_flags({})
    assert "--bind-mount" not in build_flags({"bind_mounts": []})


def test_bind_mount_coexists_with_allow_all():
    flags = build_flags(
        {
            "allow_all": True,
            "bind_mounts": [{"host": "/host", "sandbox": "/sandbox"}],
        }
    )
    assert "--allow-all" in flags
    assert "--bind-mount" in flags
    assert "/host:/sandbox" in flags


def test_bind_mount_coexists_with_no_sandbox():
    flags = build_flags(
        {
            "no_sandbox": True,
            "bind_mounts": [{"host": "/host", "sandbox": "/sandbox", "read_only": True}],
        }
    )
    assert "--no-sandbox" in flags
    assert "--bind-mount" in flags
    assert "/host:/sandbox:ro" in flags


def test_bind_mount_accepts_dataclass():
    from zerobox import BindMount, SandboxOptions

    dc = SandboxOptions(bind_mounts=[BindMount(host="/h", sandbox="/s", read_only=True)])
    dt = {"bind_mounts": [{"host": "/h", "sandbox": "/s", "read_only": True}]}
    assert build_flags(dc) == build_flags(dt)
