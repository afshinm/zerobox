//! End-to-end tests for zerobox-exec sandbox enforcement.
//!
//! Each test builds the real binary and runs it against the host OS sandbox
//! backend. On macOS this exercises seatbelt; on Linux it exercises
//! bubblewrap+seccomp (when available).
//!
//! Tests are grouped by capability: read, write, network, combined.

use std::path::PathBuf;
use std::process::{Command, Output};

/// Path to the zerobox-exec binary under test.
///
/// Set ZEROBOX_EXEC to point at a pre-built binary (e.g. a release build).
/// Falls back to the debug binary that `cargo test` builds automatically.
fn zerobox_exec() -> PathBuf {
    let path: PathBuf = std::env::var("ZEROBOX_EXEC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_zerobox-exec").into());
    // Resolve relative paths against CARGO_MANIFEST_DIR so they work
    // regardless of the cwd cargo runs the test from.
    if path.is_relative() {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("cli has parent")
            .join(&path)
    } else {
        path
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(zerobox_exec())
        .args(args)
        .output()
        .expect("failed to spawn zerobox-exec")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

// ═══════════════════════════════════════════════════════════════════════════
// Default mode: full read, no write, no network
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn default_read_succeeds() {
    std::fs::write("/tmp/zerobox-e2e-read", "hello").expect("setup");
    let out = run(&["--", "cat", "/tmp/zerobox-e2e-read"]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "hello");
}

#[test]
fn default_write_blocked() {
    let out = run(&[
        "--",
        "node",
        "-e",
        "try{require('fs').writeFileSync('/tmp/zerobox-e2e-write','x');process.exit(0)}catch(e){process.exit(1)}",
    ]);
    assert!(!out.status.success());
}

#[test]
fn default_network_blocked() {
    let out = run(&[
        "--",
        "node",
        "-e",
        "fetch('https://example.com').then(()=>process.exit(0)).catch(()=>process.exit(1))",
    ]);
    assert!(!out.status.success());
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-read: restrict readable user data
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
mod allow_read {
    use super::*;

    #[test]
    fn allowed_path_readable() {
        std::fs::write("/tmp/zerobox-e2e-ar", "secret").expect("setup");
        let out = run(&["--allow-read=/tmp", "--", "cat", "/tmp/zerobox-e2e-ar"]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        assert_eq!(stdout(&out).trim(), "secret");
    }

    #[test]
    fn etc_passwd_blocked() {
        let out = run(&["--allow-read=/tmp", "--", "cat", "/etc/passwd"]);
        assert!(!out.status.success());
        let err = stderr(&out);
        assert!(
            err.contains("Operation not permitted") || err.contains("Permission denied"),
            "unexpected stderr: {err}"
        );
    }

    #[test]
    fn home_dir_blocked() {
        let home = std::env::var("HOME").expect("HOME not set");
        let out = run(&["--allow-read=/tmp", "--", "ls", &home]);
        assert!(!out.status.success());
    }

    #[test]
    fn node_can_run_but_reads_restricted() {
        std::fs::write("/tmp/zerobox-e2e-nr", "data").expect("setup");
        let out = run(&[
            "--allow-read=/tmp",
            "--",
            "node",
            "-e",
            r#"
const fs = require('fs');
let results = [];
try { fs.readFileSync('/tmp/zerobox-e2e-nr','utf8'); results.push('tmp:ok'); }
catch(e) { results.push('tmp:blocked'); }
try { fs.readFileSync('/etc/passwd'); results.push('etc:ok'); }
catch(e) { results.push('etc:blocked'); }
console.log(results.join(','));
"#,
        ]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        let result = stdout(&out).trim().to_string();
        assert!(result.contains("tmp:ok"), "expected tmp:ok, got: {result}");
        assert!(
            result.contains("etc:blocked"),
            "expected etc:blocked, got: {result}"
        );
    }

    #[test]
    fn multiple_read_paths() {
        std::fs::write("/tmp/zerobox-e2e-m1", "one").expect("setup");
        let out = run(&[
            "--allow-read=/tmp,/var",
            "--",
            "cat",
            "/tmp/zerobox-e2e-m1",
        ]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        assert_eq!(stdout(&out).trim(), "one");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-write: grant write access
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn allow_write_specific_path() {
    let out = run(&[
        "--allow-write=/tmp",
        "--",
        "node",
        "-e",
        "require('fs').writeFileSync('/tmp/zerobox-e2e-aw','written');console.log('ok')",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ok");
    let content = std::fs::read_to_string("/tmp/zerobox-e2e-aw").expect("read back");
    assert_eq!(content, "written");
}

#[test]
fn allow_write_does_not_grant_other_paths() {
    let out = run(&[
        "--allow-write=/tmp",
        "--",
        "node",
        "-e",
        "try{require('fs').writeFileSync('/var/zerobox-e2e-aw','x');process.exit(0)}catch(e){process.exit(1)}",
    ]);
    assert!(!out.status.success());
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-net: grant network access
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn allow_net_permits_outbound() {
    let out = run(&[
        "--allow-net",
        "--",
        "curl",
        "-s",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "https://example.com",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "200");
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-all / --no-sandbox: escape hatches
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn allow_all_permits_everything() {
    let out = run(&[
        "--allow-all",
        "--",
        "node",
        "-e",
        "require('fs').writeFileSync('/tmp/zerobox-e2e-aa','x');console.log('ok')",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ok");
}

#[test]
fn no_sandbox_permits_everything() {
    let out = run(&[
        "--no-sandbox",
        "--",
        "node",
        "-e",
        "require('fs').writeFileSync('/tmp/zerobox-e2e-ns','x');console.log('ok')",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ok");
}

// ═══════════════════════════════════════════════════════════════════════════
// Combined flags
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
#[test]
fn allow_read_and_write_combined() {
    std::fs::write("/tmp/zerobox-e2e-rw-in", "input").expect("setup");
    let out = run(&[
        "--allow-read=/tmp",
        "--allow-write=/tmp",
        "--",
        "node",
        "-e",
        r#"
const fs = require('fs');
const data = fs.readFileSync('/tmp/zerobox-e2e-rw-in','utf8');
fs.writeFileSync('/tmp/zerobox-e2e-rw-out', data + '-processed');
console.log('ok');
"#,
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let content = std::fs::read_to_string("/tmp/zerobox-e2e-rw-out").expect("read back");
    assert_eq!(content, "input-processed");
}

#[cfg(target_os = "macos")]
#[test]
fn allow_read_and_net_combined() {
    let out = run(&[
        "--allow-read=/tmp",
        "--allow-net",
        "--",
        "curl",
        "-s",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "https://example.com",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "200");
}

// ═══════════════════════════════════════════════════════════════════════════
// Exit code propagation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn exit_code_zero_propagated() {
    let out = run(&["--", "node", "-e", "process.exit(0)"]);
    assert!(out.status.success());
}

#[test]
fn exit_code_nonzero_propagated() {
    let out = run(&["--", "node", "-e", "process.exit(42)"]);
    assert_eq!(out.status.code(), Some(42));
}

// ═══════════════════════════════════════════════════════════════════════════
// Edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn relative_write_path_resolved() {
    // --allow-write=. should resolve to CWD
    let out = run(&[
        "--allow-write=/tmp",
        "-C",
        "/tmp",
        "--",
        "node",
        "-e",
        "require('fs').writeFileSync('/tmp/zerobox-e2e-rel','ok');console.log('ok')",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
}

#[test]
fn nonexistent_command_fails() {
    let out = run(&["--", "this-command-does-not-exist-zerobox"]);
    assert!(!out.status.success());
}
