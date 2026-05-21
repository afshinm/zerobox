use crate::support::*;

/// End-to-end tests for `--bind-mount`.
///
/// The sandbox runtime requires user-namespace privileges that are not
/// available in every CI environment; tests probe the actual bwrap path and
/// skip if the host is unable to set up the loopback / user namespace. CI
/// running with namespaces enabled exercises the real mount.
fn user_namespaces_available() -> bool {
    let out = run(&["--", "true"]);
    !stderr(&out).contains("Failed RTM_NEWADDR")
        && !stderr(&out).contains("No permissions to create a new namespace")
}

#[cfg(target_os = "linux")]
#[test]
fn bind_mount_exposes_host_contents_at_sandbox_path() {
    if !user_namespaces_available() {
        return;
    }
    let src = tempfile::tempdir().expect("src dir");
    let dst_parent = tempfile::tempdir().expect("dst parent");
    let dst = dst_parent.path().join("destination-target");
    std::fs::create_dir_all(&dst).expect("create dst placeholder");
    let marker = src.path().join("marker.txt");
    std::fs::write(&marker, b"hello-from-host").expect("write marker");

    let spec = format!("{}:{}", src.path().display(), dst.display());
    let probe = format!("cat {}/marker.txt", dst.display());
    let out = run(&[
        "--bind-mount",
        &spec,
        "--allow-read",
        dst.to_str().unwrap(),
        "--",
        "sh",
        "-c",
        &probe,
    ]);
    assert!(
        out.status.success(),
        "stderr: {}\nstdout: {}",
        stderr(&out),
        stdout(&out)
    );
    assert_eq!(stdout(&out).trim(), "hello-from-host");
}

#[cfg(target_os = "linux")]
#[test]
fn bind_mount_read_only_rejects_writes() {
    if !user_namespaces_available() {
        return;
    }
    let src = tempfile::tempdir().expect("src dir");
    let dst_parent = tempfile::tempdir().expect("dst parent");
    let dst = dst_parent.path().join("ro-dst");
    std::fs::create_dir_all(&dst).expect("create dst placeholder");
    let spec = format!("{}:{}:ro", src.path().display(), dst.display());
    let cmd = format!(
        "echo nope > {}/file && echo OK || echo BLOCKED",
        dst.display()
    );
    let out = run(&[
        "--bind-mount",
        &spec,
        "--allow-read",
        dst.to_str().unwrap(),
        "--",
        "sh",
        "-c",
        &cmd,
    ]);
    assert!(
        stdout(&out).contains("BLOCKED"),
        "ro bind mount should reject writes; stdout: {}, stderr: {}",
        stdout(&out),
        stderr(&out)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bind_mount_forbidden_roots_rejected_before_spawn() {
    let src = tempfile::tempdir().expect("src dir");
    for forbidden in &["/", "/proc", "/sys", "/dev"] {
        let spec = format!("{}:{}", src.path().display(), forbidden);
        let out = run(&["--bind-mount", &spec, "--", "true"]);
        assert!(
            !out.status.success(),
            "expected forbidden root {forbidden} to be rejected; stderr: {}",
            stderr(&out),
        );
        assert!(
            stderr(&out).contains("--bind-mount") && stderr(&out).contains("forbidden"),
            "expected clear forbidden-root error for {forbidden}, got: {}",
            stderr(&out),
        );
    }
}

#[cfg(target_os = "linux")]
#[test]
fn bind_mount_missing_host_dir_rejected_before_spawn() {
    let spec = "/nonexistent/path/for/bind/mount/test:/tmp/dst-missing";
    let out = run(&["--bind-mount", spec, "--", "true"]);
    assert!(
        !out.status.success(),
        "missing host dir should be rejected; stderr: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("does not exist"),
        "missing host dir error not surfaced, got: {}",
        stderr(&out)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bind_mount_relative_sandbox_path_rejected() {
    let src = tempfile::tempdir().expect("src dir");
    let spec = format!("{}:relative/path", src.path().display());
    let out = run(&["--bind-mount", &spec, "--", "true"]);
    assert!(
        !out.status.success(),
        "relative SANDBOX should be rejected; stderr: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("absolute"),
        "expected absolute-path error, got: {}",
        stderr(&out)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn bind_mount_host_file_not_directory_rejected() {
    let src = tempfile::tempdir().expect("src dir");
    let file = src.path().join("regular-file");
    std::fs::write(&file, b"x").expect("write file");
    let spec = format!("{}:/tmp/dst-file-bind", file.display());
    let out = run(&["--bind-mount", &spec, "--", "true"]);
    assert!(
        !out.status.success(),
        "non-directory host should be rejected; stderr: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("not a directory"),
        "expected not-a-directory error, got: {}",
        stderr(&out)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn bind_mount_warns_and_runs_on_macos() {
    let src = tempfile::tempdir().expect("src dir");
    let spec = format!("{}:/tmp/dst-macos", src.path().display());
    let out = run(&["--bind-mount", &spec, "--", "echo", "hello"]);
    assert!(
        out.status.success(),
        "command should still run; stderr: {}",
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("--bind-mount is a no-op") && stderr(&out).contains("macOS"),
        "expected macOS no-op warning, got: {}",
        stderr(&out)
    );
    assert_eq!(stdout(&out).trim(), "hello");
}
