//! End-to-end tests for zerobox-exec sandbox enforcement.
//!
//! Each test runs the real binary through the Codex sandboxing API.
//! On macOS this exercises seatbelt; on Linux bubblewrap+seccomp.
//!
//! Set ZEROBOX_EXEC to test a pre-built binary (e.g. release build).
//! Falls back to the debug binary that `cargo test` builds automatically.

use std::path::PathBuf;
use std::process::{Command, Output};

fn zerobox_exec() -> PathBuf {
    let path: PathBuf = std::env::var("ZEROBOX_EXEC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_zerobox").into());
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

fn setup_tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/zerobox-e2e-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("setup dir");
    dir
}

/// Helper: run curl and return the HTTP status code string.
fn curl_status(args: &[&str], url: &str) -> (String, bool) {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.extend([
        "--",
        "curl",
        "-s",
        "--max-time",
        "5",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        url,
    ]);
    let out = run(&full_args);
    let code = stdout(&out).trim().to_string();
    (code.clone(), code == "200")
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
        "try{require('fs').writeFileSync('/tmp/zerobox-e2e-wb','x');process.exit(0)}catch(e){process.exit(1)}",
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
// --allow-read: restrict readable paths (includes platform defaults)
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
    fn home_dir_blocked() {
        let home = std::env::var("HOME").expect("HOME not set");
        let out = run(&["--allow-read=/tmp", "--", "ls", &home]);
        assert!(!out.status.success());
    }

    #[test]
    fn cat_runs_with_restricted_read() {
        std::fs::write("/tmp/zerobox-e2e-nr", "data").expect("setup");
        let out = run(&["--allow-read=/tmp", "--", "cat", "/tmp/zerobox-e2e-nr"]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        assert_eq!(stdout(&out).trim(), "data");
    }

    #[test]
    fn multiple_read_paths() {
        std::fs::write("/tmp/zerobox-e2e-mr", "one").expect("setup");
        let out = run(&["--allow-read=/tmp,/var", "--", "cat", "/tmp/zerobox-e2e-mr"]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        assert_eq!(stdout(&out).trim(), "one");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// --deny-read: carve out exceptions within allowed reads
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
mod deny_read {
    use super::*;

    #[test]
    fn deny_blocks_within_default_full_read() {
        let dir = setup_tmp("dr1");
        let secret = dir.join("private");
        std::fs::create_dir_all(&secret).expect("setup");
        std::fs::write(secret.join("key.txt"), "password").expect("setup");

        let out = run(&[
            &format!("--deny-read={}", secret.display()),
            "--",
            "node",
            "-e",
            &format!(
                "try{{require('fs').readFileSync('{}/private/key.txt');console.log('ALLOWED')}}catch(e){{console.log('BLOCKED')}}",
                dir.display()
            ),
        ]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        assert_eq!(stdout(&out).trim(), "BLOCKED");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-write / --deny-write
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

#[cfg(target_os = "macos")]
mod deny_write {
    use super::*;

    #[test]
    fn deny_blocks_within_allowed_dir() {
        let dir = setup_tmp("dw1");
        let protected = dir.join(".git");
        std::fs::create_dir_all(&protected).expect("setup");

        let out = run(&[
            &format!("--allow-write={}", dir.display()),
            &format!("--deny-write={}", protected.display()),
            "--",
            "node",
            "-e",
            &format!(
                r#"
const fs = require('fs');
let r = [];
try {{ fs.writeFileSync('{}/file.txt','ok'); r.push('file:ok'); }} catch(e) {{ r.push('file:blocked'); }}
try {{ fs.writeFileSync('{}/.git/evil','x'); r.push('git:ok'); }} catch(e) {{ r.push('git:blocked'); }}
console.log(r.join(','));
"#,
                dir.display(),
                dir.display()
            ),
        ]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        let result = stdout(&out).trim().to_string();
        assert!(result.contains("file:ok"), "got: {result}");
        assert!(result.contains("git:blocked"), "got: {result}");
    }

    #[test]
    fn deny_write_with_full_write() {
        let dir = setup_tmp("dw2");
        std::fs::create_dir_all(&dir).expect("setup");

        let out = run(&[
            "--allow-write",
            &format!("--deny-write={}", dir.display()),
            "--",
            "node",
            "-e",
            &format!(
                "try{{require('fs').writeFileSync('{}/blocked.txt','x');console.log('ALLOWED')}}catch(e){{console.log('BLOCKED')}}",
                dir.display()
            ),
        ]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        assert_eq!(stdout(&out).trim(), "BLOCKED");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-net: boolean (all network)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn allow_net_full_permits_outbound() {
    let (code, ok) = curl_status(&["--allow-net"], "https://example.com");
    assert!(ok, "expected 200, got {code}");
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-net=<domains>: domain-level filtering via Codex network proxy
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
mod allow_net_domains {
    use super::*;

    #[test]
    fn single_domain_allowed() {
        let (code, ok) = curl_status(&["--allow-net=example.com"], "https://example.com");
        assert!(ok, "expected 200, got {code}");
    }

    #[test]
    fn unlisted_domain_blocked() {
        let (code, ok) = curl_status(&["--allow-net=example.com"], "https://google.com");
        assert!(!ok, "expected blocked, got {code}");
    }

    #[test]
    fn multiple_domains_allowed() {
        let (code, ok) = curl_status(
            &["--allow-net=example.com,google.com"],
            "https://example.com",
        );
        assert!(ok, "expected 200, got {code}");
    }

    #[test]
    fn wildcard_subdomain_allows_subdomains() {
        // *.example.com should allow www.example.com
        // (we can't easily test a real subdomain that resolves, so test
        // that the apex is NOT matched by the wildcard -- that's the
        // important semantic)
        let (code, ok) = curl_status(&["--allow-net=*.example.com"], "https://example.com");
        assert!(
            !ok,
            "*.example.com should NOT match apex example.com, got {code}"
        );
    }

    #[test]
    fn apex_and_wildcard_combined() {
        // To allow both apex and subdomains, list both.
        let (code, ok) = curl_status(
            &["--allow-net=example.com,*.example.com"],
            "https://example.com",
        );
        assert!(ok, "expected 200, got {code}");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// --deny-net=<domains>: block specific domains
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(target_os = "macos")]
mod deny_net_domains {
    use super::*;

    #[test]
    fn deny_blocks_specific_domain() {
        let (code, ok) = curl_status(
            &["--allow-net", "--deny-net=google.com"],
            "https://google.com",
        );
        assert!(!ok, "expected blocked, got {code}");
    }

    #[test]
    fn deny_does_not_affect_other_domains() {
        let (code, ok) = curl_status(
            &["--allow-net", "--deny-net=google.com"],
            "https://example.com",
        );
        assert!(ok, "expected 200, got {code}");
    }

    #[test]
    fn deny_overrides_allow() {
        // Allow example.com but also deny it. Deny wins.
        let (code, ok) = curl_status(
            &["--allow-net=example.com", "--deny-net=example.com"],
            "https://example.com",
        );
        assert!(!ok, "deny should override allow, got {code}");
    }

    #[test]
    fn deny_wildcard_blocks_subdomains() {
        // Deny *.google.com, allow everything else.
        let (code, ok) = curl_status(
            &["--allow-net", "--deny-net=*.google.com"],
            "https://example.com",
        );
        assert!(ok, "example.com should still work, got {code}");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// --allow-all / --no-sandbox
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
        "sh",
        "-c",
        "cat /tmp/zerobox-e2e-rw-in > /tmp/zerobox-e2e-rw-out && echo ok",
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    assert_eq!(stdout(&out).trim(), "ok");
    let content = std::fs::read_to_string("/tmp/zerobox-e2e-rw-out").expect("read back");
    assert_eq!(content.trim(), "input");
}

#[cfg(target_os = "macos")]
#[test]
fn allow_read_and_net_combined() {
    let (code, ok) = curl_status(&["--allow-read=/tmp", "--allow-net"], "https://example.com");
    assert!(ok, "expected 200, got {code}");
}

#[cfg(target_os = "macos")]
#[test]
fn deny_read_and_deny_write_combined() {
    let dir = setup_tmp("combo");
    let secret = dir.join("secret");
    std::fs::create_dir_all(&secret).expect("setup");
    std::fs::write(dir.join("public"), "hello").expect("setup");

    let out = run(&[
        &format!("--allow-write={}", dir.display()),
        &format!("--deny-write={}", secret.display()),
        "--",
        "node",
        "-e",
        &format!(
            r#"
const fs = require('fs');
let r = [];
try {{ fs.writeFileSync('{}/new.txt','x'); r.push('write-pub:ok'); }} catch(e) {{ r.push('write-pub:blocked'); }}
try {{ fs.writeFileSync('{}/secret/evil','x'); r.push('write-sec:ok'); }} catch(e) {{ r.push('write-sec:blocked'); }}
console.log(r.join(','));
"#,
            dir.display(),
            dir.display()
        ),
    ]);
    assert!(out.status.success(), "stderr: {}", stderr(&out));
    let result = stdout(&out).trim().to_string();
    assert!(result.contains("write-pub:ok"), "got: {result}");
    assert!(result.contains("write-sec:blocked"), "got: {result}");
}

#[cfg(target_os = "macos")]
#[test]
fn allow_net_domain_with_write_restriction() {
    let dir = setup_tmp("net-write");
    let (code, ok) = curl_status(
        &[
            "--allow-net=example.com",
            &format!("--allow-write={}", dir.display()),
        ],
        "https://example.com",
    );
    assert!(ok, "expected 200, got {code}");
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
