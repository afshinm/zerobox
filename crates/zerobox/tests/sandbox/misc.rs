use crate::support::*;

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
        "sh",
        "-c",
        "echo x > /tmp/zerobox-e2e-wb 2>/dev/null && echo OK || echo BLOCKED",
    ]);
    assert!(
        stdout(&out).contains("BLOCKED"),
        "write should be blocked, got: {}",
        stdout(&out)
    );
}

#[test]
fn default_network_blocked() {
    let (code, ok) = curl_status(&[], "https://example.com");
    assert!(!ok, "network should be blocked, got {code}");
}

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

#[test]
fn allow_read_and_net_combined() {
    let (code, ok) = curl_status(
        &[
            "--allow-read=/tmp,/run",
            "--allow-write=/tmp",
            "--allow-net",
        ],
        "https://example.com",
    );
    assert!(ok, "expected 200, got {code}");
}

#[test]
fn deny_read_and_deny_write_combined() {
    let dir = setup_tmp("combo");
    let secret = dir.join("secret");
    std::fs::create_dir_all(&secret).expect("setup");
    std::fs::write(dir.join("public"), "hello").expect("setup");

    let out = run(&[
        "--profile",
        "workspace",
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

#[test]
fn exit_code_zero_propagated() {
    let out = run(&[
        "--profile",
        "workspace",
        "--",
        "node",
        "-e",
        "process.exit(0)",
    ]);
    assert!(out.status.success());
}

#[test]
fn exit_code_nonzero_propagated() {
    let out = run(&[
        "--profile",
        "workspace",
        "--",
        "node",
        "-e",
        "process.exit(42)",
    ]);
    assert_eq!(out.status.code(), Some(42));
}

#[test]
fn relative_write_path_resolved() {
    let out = run(&[
        "--profile",
        "workspace",
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

mod strict_flag {
    use super::*;

    #[test]
    fn strict_works_when_namespaces_available() {
        let out = run(&["--strict-sandbox", "--", "echo", "hello"]);
        assert!(
            out.status.success(),
            "strict should work when namespaces are available, stderr: {}",
            stderr(&out)
        );
        assert_eq!(stdout(&out).trim(), "hello");
    }

    #[test]
    fn strict_with_allow_write() {
        let out = run(&[
            "--strict-sandbox",
            "--allow-write=/tmp",
            "--",
            "sh",
            "-c",
            "echo ok > /tmp/zerobox-strict-test && cat /tmp/zerobox-strict-test",
        ]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        assert_eq!(stdout(&out).trim(), "ok");
    }
}
