pub use std::path::PathBuf;
pub use std::process::{Command, Output};

pub fn zerobox_exec() -> PathBuf {
    let path: PathBuf = std::env::var("ZEROBOX_EXEC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env!("CARGO_BIN_EXE_zerobox").into());
    if path.is_relative() {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .join(&path)
    } else {
        path
    }
}

pub fn run(args: &[&str]) -> Output {
    Command::new(zerobox_exec())
        .args(args)
        .output()
        .expect("failed to spawn zerobox")
}

pub fn run_with_home(home: &std::path::Path, args: &[&str]) -> Output {
    Command::new(zerobox_exec())
        .args(args)
        .env("ZEROBOX_HOME", home)
        .output()
        .expect("failed to spawn zerobox")
}

pub fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

pub fn temp_dir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create temp dir")
}

pub fn setup_tmp(name: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/zerobox-e2e-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("setup dir");
    dir
}

pub fn curl_status(args: &[&str], url: &str) -> (String, bool) {
    let mut full_args: Vec<&str> = args.to_vec();
    full_args.extend([
        "--",
        "curl",
        "-sL",
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
