use crate::support::*;

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
    fn unallowed_path_blocked() {
        let home = std::env::var("HOME").expect("HOME not set");
        let dir = PathBuf::from(&home).join(".zerobox-e2e-blocked");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("setup");
        std::fs::write(dir.join("secret.txt"), "hidden").expect("setup");
        let out = run(&[
            "--allow-read=/var",
            "--",
            "cat",
            &dir.join("secret.txt").to_string_lossy(),
        ]);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            !out.status.success(),
            "should be blocked, stdout: {}",
            stdout(&out)
        );
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

    #[cfg(target_os = "linux")]
    #[test]
    fn deny_blocks_within_explicit_allow_read() {
        let dir = setup_tmp("dr2");
        let secret = dir.join("secret");
        std::fs::create_dir_all(&secret).expect("setup");
        std::fs::write(dir.join("public.txt"), "visible").expect("setup");
        std::fs::write(secret.join("key.txt"), "hidden").expect("setup");

        let out = run(&[
            &format!("--allow-read={}", dir.display()),
            &format!("--deny-read={}", secret.display()),
            "--",
            "sh",
            "-c",
            &format!(
                "cat {}/public.txt 2>/dev/null && cat {}/secret/key.txt 2>/dev/null || echo DENIED",
                dir.display(),
                dir.display()
            ),
        ]);
        assert!(out.status.success(), "stderr: {}", stderr(&out));
        let out_str = stdout(&out);
        assert!(
            out_str.contains("visible"),
            "public file should be readable, got: {out_str}"
        );
        assert!(
            !out_str.contains("hidden"),
            "secret file should be blocked, got: {out_str}"
        );
    }
}
