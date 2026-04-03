use crate::support::*;

#[test]
fn secret_sets_placeholder_in_env() {
    let out = run(&[
        "--allow-all",
        "--secret",
        "MY_TOKEN=real-value",
        "--secret-host",
        "MY_TOKEN=httpbin.org",
        "--",
        "sh",
        "-c",
        "echo $MY_TOKEN",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let val = stdout(&out).trim().to_string();
    assert!(
        val.starts_with("ZEROBOX_SECRET_"),
        "expected placeholder, got: {val}"
    );
    assert_ne!(val, "real-value", "real value must not be visible");
}

#[test]
fn secret_without_host_works() {
    let out = run(&[
        "--allow-all",
        "--secret",
        "TOKEN=abc",
        "--",
        "sh",
        "-c",
        "echo $TOKEN",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let val = stdout(&out).trim().to_string();
    assert!(val.starts_with("ZEROBOX_SECRET_"));
}

#[test]
fn secret_bad_format_fails() {
    let out = run(&["--no-sandbox", "--secret", "NOEQUALS", "--", "echo", "hi"]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("KEY=VALUE"),
        "should hint at format, got: {}",
        stderr(&out)
    );
}

#[test]
fn secret_duplicate_key_fails() {
    let out = run(&[
        "--allow-all",
        "--secret",
        "K=v1",
        "--secret",
        "K=v2",
        "--",
        "echo",
        "hi",
    ]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("duplicate"),
        "should report duplicate, got: {}",
        stderr(&out)
    );
}

#[test]
fn secret_host_unknown_key_fails() {
    let out = run(&[
        "--allow-all",
        "--secret",
        "A=val",
        "--secret-host",
        "B=host.com",
        "--",
        "echo",
        "hi",
    ]);
    assert!(!out.status.success());
    assert!(
        stderr(&out).contains("unknown"),
        "should report unknown key, got: {}",
        stderr(&out)
    );
}

#[test]
fn secret_combined_with_env() {
    let out = run(&[
        "--allow-all",
        "--secret",
        "TOKEN=secret",
        "--secret-host",
        "TOKEN=httpbin.org",
        "--env",
        "FOO=bar",
        "--",
        "sh",
        "-c",
        "echo FOO=$FOO TOKEN=$TOKEN",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let s = stdout(&out).trim().to_string();
    assert!(s.contains("FOO=bar"), "env should work, got: {s}");
    assert!(
        s.contains("TOKEN=ZEROBOX_SECRET_"),
        "secret should be placeholder, got: {s}"
    );
}

#[test]
fn secret_host_only_enables_that_host() {
    let (code, ok) = curl_status(
        &[
            "--secret",
            "TOKEN=val",
            "--secret-host",
            "TOKEN=httpbin.org",
        ],
        "https://example.com",
    );
    assert!(!ok, "example.com should be blocked, got {code}");
}
