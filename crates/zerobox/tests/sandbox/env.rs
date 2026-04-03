use crate::support::*;

#[test]
fn default_env_has_path() {
    let out = run(&["--allow-all", "--", "sh", "-c", "echo $PATH"]);
    assert!(out.status.success());
    assert!(!stdout(&out).trim().is_empty(), "PATH should be set");
}

#[test]
fn default_env_has_home() {
    let out = run(&["--allow-all", "--", "sh", "-c", "echo $HOME"]);
    assert!(out.status.success());
    assert!(!stdout(&out).trim().is_empty(), "HOME should be set");
}

#[test]
fn default_env_excludes_non_essential_vars() {
    let out = Command::new(zerobox_exec())
        .args(["--allow-all", "--", "sh", "-c", "echo $ZEROBOX_TEST_VAR"])
        .env("ZEROBOX_TEST_VAR", "should_not_appear")
        .output()
        .expect("spawn");
    assert!(out.status.success());
    assert_eq!(
        stdout(&out).trim(),
        "",
        "custom var should not be inherited"
    );
}

#[test]
fn default_env_is_minimal() {
    let out = run(&["--allow-all", "--", "env"]);
    assert!(out.status.success());
    let count = stdout(&out).lines().count();
    assert!(count <= 10, "expected minimal env, got {count} vars");
}

#[test]
fn env_sets_explicit_var() {
    let out = run(&[
        "--allow-all",
        "--env",
        "FOO=bar",
        "--",
        "sh",
        "-c",
        "echo $FOO",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "bar");
}

#[test]
fn env_multiple_vars() {
    let out = run(&[
        "--allow-all",
        "--env",
        "A=1",
        "--env",
        "B=2",
        "--",
        "sh",
        "-c",
        "echo $A $B",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "1 2");
}

#[test]
fn env_value_with_equals() {
    let out = run(&[
        "--allow-all",
        "--env",
        "DATA=key=value=extra",
        "--",
        "sh",
        "-c",
        "echo $DATA",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "key=value=extra");
}

#[test]
fn env_empty_value() {
    let out = run(&[
        "--allow-all",
        "--env",
        "EMPTY=",
        "--",
        "sh",
        "-c",
        "echo \"x${EMPTY}x\"",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "xx");
}

#[test]
fn env_overrides_inherited() {
    let out = run(&[
        "--allow-all",
        "--allow-env",
        "--env",
        "HOME=/custom",
        "--",
        "sh",
        "-c",
        "echo $HOME",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "/custom");
}

#[test]
fn allow_env_bare_inherits_all() {
    let out = Command::new(zerobox_exec())
        .args([
            "--allow-all",
            "--allow-env",
            "--",
            "sh",
            "-c",
            "echo $ZEROBOX_TEST_ALL",
        ])
        .env("ZEROBOX_TEST_ALL", "visible")
        .output()
        .expect("spawn");
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "visible");
}

#[test]
fn allow_env_bare_has_many_vars() {
    let out = run(&["--allow-all", "--allow-env", "--", "env"]);
    assert!(out.status.success());
    let count = stdout(&out).lines().count();
    assert!(
        count > 10,
        "expected many vars with --allow-env, got {count}"
    );
}

#[test]
fn allow_env_specific_keys() {
    let out = run(&["--allow-all", "--allow-env=PATH", "--", "env"]);
    assert!(out.status.success());
    let lines = stdout(&out);
    assert!(lines.contains("PATH="), "PATH should be present");
    assert!(!lines.contains("HOME="), "HOME should not be present");
}

#[test]
fn allow_env_specific_with_env_override() {
    let out = run(&[
        "--allow-all",
        "--allow-env=PATH",
        "--env",
        "FOO=added",
        "--",
        "sh",
        "-c",
        "echo FOO=$FOO HOME=$HOME",
    ]);
    assert!(out.status.success());
    let s = stdout(&out).trim().to_string();
    assert!(
        s.contains("FOO=added"),
        "explicit --env should survive, got: {s}"
    );
    assert!(s.contains("HOME="), "HOME should be empty");
    assert!(!s.contains("HOME=/"), "HOME should not have a value");
}

#[test]
fn deny_env_removes_var() {
    let out = run(&[
        "--allow-all",
        "--allow-env",
        "--deny-env=HOME",
        "--",
        "sh",
        "-c",
        "echo \"HOME=$HOME\"",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "HOME=");
}

#[test]
fn deny_env_takes_precedence_over_allow() {
    let out = run(&[
        "--allow-all",
        "--allow-env=PATH,HOME",
        "--deny-env=HOME",
        "--",
        "env",
    ]);
    assert!(out.status.success());
    let lines = stdout(&out);
    assert!(lines.contains("PATH="), "PATH should survive");
    assert!(!lines.contains("HOME="), "HOME should be denied");
}

#[test]
fn deny_env_does_not_block_explicit_env() {
    let out = run(&[
        "--allow-all",
        "--allow-env",
        "--deny-env=FOO",
        "--env",
        "FOO=override",
        "--",
        "sh",
        "-c",
        "echo $FOO",
    ]);
    assert!(out.status.success());
    assert_eq!(stdout(&out).trim(), "override");
}

#[test]
fn deny_env_multiple_keys() {
    let out = run(&[
        "--allow-all",
        "--allow-env",
        "--deny-env=HOME,USER",
        "--",
        "sh",
        "-c",
        "echo \"HOME=$HOME USER=$USER\"",
    ]);
    assert!(out.status.success());
    let s = stdout(&out).trim().to_string();
    assert_eq!(s, "HOME= USER=");
}

#[test]
fn env_without_equals_fails() {
    let out = run(&["--allow-all", "--env", "BADFORMAT", "--", "echo", "hi"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("KEY=VALUE"),
        "should hint at format, got: {}",
        stderr(&out)
    );
}
