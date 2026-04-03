use crate::support::*;
use std::fs;

#[test]
fn snapshot_records_changes() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--snapshot",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo modified > {}/file.txt", dir.path().display()),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("1 modified"),
        "expected change summary, got: {err}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "modified"
    );
}

#[test]
fn restore_undoes_changes() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!(
                "echo modified > {}/file.txt; echo new > {}/new.txt",
                dir.path().display(),
                dir.path().display()
            ),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("restored"),
        "expected restore message, got: {err}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "original"
    );
    assert!(!dir.path().join("new.txt").exists());
}

#[test]
fn snapshot_no_changes_reports_none() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "unchanged").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--snapshot",
            &format!("--snapshot-path={}", dir.path().display()),
            "--",
            "true",
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(err.contains("no changes detected"), "got: {err}");
}

#[test]
fn snapshot_session_discoverable_via_list() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "data").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--snapshot",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo changed > {}/file.txt", dir.path().display()),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));

    let list = run_with_home(home.path(), &["snapshot", "list"]);
    assert!(list.status.success(), "stderr: {}", stderr(&list));
    let list_out = stdout(&list);
    assert!(
        list_out.contains("sh"),
        "expected session in list, got: {list_out}"
    );
}

#[test]
fn snapshot_subcommand_restore_works() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--snapshot",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo changed > {}/file.txt", dir.path().display()),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "changed"
    );

    let list = run_with_home(home.path(), &["snapshot", "list"]);
    let list_out = stdout(&list);
    let session_id = list_out
        .lines()
        .nth(1) // skip header
        .and_then(|l| l.split_whitespace().next())
        .expect("no session in list");

    let restore = run_with_home(home.path(), &["snapshot", "restore", session_id]);
    assert!(restore.status.success(), "stderr: {}", stderr(&restore));
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "original"
    );
}

#[test]
fn restore_with_excluded_dir_preserves_it() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();
    fs::create_dir_all(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".git/HEAD"), "ref: refs/heads/main").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo modified > {}/file.txt", dir.path().display()),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "original"
    );
    assert!(dir.path().join(".git/HEAD").exists());
}

#[test]
fn restore_does_not_touch_files_outside_tracked_path() {
    let tracked = temp_dir();
    let outside = temp_dir();
    let home = temp_dir();
    fs::write(tracked.path().join("a.txt"), "tracked").unwrap();
    fs::write(outside.path().join("b.txt"), "untouched").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={}", tracked.path().display()),
            &format!("--allow-write={}", tracked.path().display()),
            &format!("--allow-write={}", outside.path().display()),
            "--",
            "sh",
            "-c",
            &format!(
                "echo changed > {}/a.txt; echo new > {}/b.txt",
                tracked.path().display(),
                outside.path().display()
            ),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        fs::read_to_string(tracked.path().join("a.txt"))
            .unwrap()
            .trim(),
        "tracked"
    );
    assert_eq!(
        fs::read_to_string(outside.path().join("b.txt"))
            .unwrap()
            .trim(),
        "new"
    );
}

#[test]
fn restore_still_runs_when_command_fails() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo modified > {}/file.txt; exit 1", dir.path().display()),
        ],
    );
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(err.contains("restored"), "expected restore, got: {err}");
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "original"
    );
}

#[test]
fn snapshot_exclude_skips_patterns() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("keep.txt"), "keep").unwrap();
    fs::create_dir_all(dir.path().join("build")).unwrap();
    fs::write(dir.path().join("build/out.js"), "build output").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--snapshot",
            "--snapshot-exclude=build",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!(
                "echo changed > {}/keep.txt; echo new > {}/build/extra.js",
                dir.path().display(),
                dir.path().display()
            ),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("1 modified"),
        "expected 1 modified (keep.txt only), got: {err}"
    );
    assert!(
        !err.contains("build"),
        "build dir should not appear in changes: {err}"
    );
}

#[test]
fn snapshot_clean_removes_sessions() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("f.txt"), "x").unwrap();

    run_with_home(
        home.path(),
        &[
            "--snapshot",
            &format!("--snapshot-path={}", dir.path().display()),
            "--",
            "true",
        ],
    );

    let list_before = run_with_home(home.path(), &["snapshot", "list"]);
    assert!(stdout(&list_before).lines().count() >= 2);

    let clean = run_with_home(home.path(), &["snapshot", "clean", "--older-than=0"]);
    assert!(clean.status.success(), "stderr: {}", stderr(&clean));
    let clean_err = stderr(&clean);
    assert!(clean_err.contains("Removed"), "got: {clean_err}");

    let list_after = run_with_home(home.path(), &["snapshot", "list"]);
    let after_err = stderr(&list_after);
    assert!(after_err.contains("No snapshots found"), "got: {after_err}");
}

#[test]
fn snapshot_diff_shows_changes() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();

    run_with_home(
        home.path(),
        &[
            "--snapshot",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo changed > {}/file.txt", dir.path().display()),
        ],
    );

    let list = run_with_home(home.path(), &["snapshot", "list"]);
    let list_out = stdout(&list);
    let session_id = list_out
        .lines()
        .nth(1)
        .and_then(|l| l.split_whitespace().next())
        .expect("no session in list");

    let diff = run_with_home(home.path(), &["snapshot", "diff", session_id]);
    assert!(diff.status.success(), "stderr: {}", stderr(&diff));
    let diff_err = stderr(&diff);
    assert!(diff_err.contains("1 modified"), "got: {diff_err}");
}

#[test]
fn restore_reverts_file_replaced_by_directory() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("config"), "i am a file").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!(
                "rm {d}/config && mkdir -p {d}/config/sub && echo x > {d}/config/sub/f",
                d = dir.path().display()
            ),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(dir.path().join("config").is_file());
    assert_eq!(
        fs::read_to_string(dir.path().join("config"))
            .unwrap()
            .trim(),
        "i am a file"
    );
}

#[test]
fn restore_recreates_deleted_file() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("precious.txt"), "do not lose me").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("rm {}/precious.txt", dir.path().display()),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert!(dir.path().join("precious.txt").exists());
    assert_eq!(
        fs::read_to_string(dir.path().join("precious.txt"))
            .unwrap()
            .trim(),
        "do not lose me"
    );
}

#[test]
fn snapshot_without_allow_write_records_no_changes() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--snapshot",
            &format!("--snapshot-path={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo hacked > {}/file.txt || true", dir.path().display()),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let err = stderr(&out);
    assert!(
        err.contains("no changes detected"),
        "writes should be blocked, got: {err}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "original"
    );
}

#[test]
fn snapshot_restore_nonexistent_session_fails() {
    let home = temp_dir();
    let out = run_with_home(home.path(), &["snapshot", "restore", "does-not-exist"]);
    assert!(!out.status.success());
    let err = stderr(&out);
    assert!(
        err.contains("session not found"),
        "expected not found error, got: {err}"
    );
}

#[test]
fn restore_preserves_command_exit_code() {
    let dir = temp_dir();
    let home = temp_dir();
    fs::write(dir.path().join("file.txt"), "original").unwrap();

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={}", dir.path().display()),
            &format!("--allow-write={}", dir.path().display()),
            "--",
            "sh",
            "-c",
            &format!("echo changed > {}/file.txt; exit 42", dir.path().display()),
        ],
    );
    assert_eq!(out.status.code(), Some(42));
    assert_eq!(
        fs::read_to_string(dir.path().join("file.txt"))
            .unwrap()
            .trim(),
        "original"
    );
}

#[test]
fn snapshot_path_with_dotdot_works() {
    let dir = temp_dir();
    let home = temp_dir();
    let subdir = dir.path().join("sub");
    fs::create_dir_all(&subdir).unwrap();
    fs::write(subdir.join("file.txt"), "original").unwrap();

    // Pass a path containing .. that resolves to subdir.
    let dotdot_path = format!("{}/sub/../sub", dir.path().display());

    let out = run_with_home(
        home.path(),
        &[
            "--restore",
            &format!("--snapshot-path={dotdot_path}"),
            &format!("--allow-write={}", subdir.display()),
            "--",
            "sh",
            "-c",
            &format!("echo changed > {}/file.txt", subdir.display()),
        ],
    );
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(
        fs::read_to_string(subdir.join("file.txt")).unwrap().trim(),
        "original"
    );
}
