use crate::support::*;

#[test]
fn allow_write_specific_path() {
    let out = run(&[
        "--profile",
        "workspace",
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
        "--profile",
        "workspace",
        "--allow-write=/tmp",
        "--",
        "node",
        "-e",
        "try{require('fs').writeFileSync('/var/zerobox-e2e-aw','x');process.exit(0)}catch(e){process.exit(1)}",
    ]);
    assert!(!out.status.success());
}

mod deny_write {
    use super::*;

    #[test]
    fn deny_blocks_within_allowed_dir() {
        let dir = setup_tmp("dw1");
        let protected = dir.join(".git");
        std::fs::create_dir_all(&protected).expect("setup");

        let out = run(&[
            "--profile",
            "workspace",
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
}
