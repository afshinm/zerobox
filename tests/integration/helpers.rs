use std::path::PathBuf;
use std::time::Duration;

pub fn daemon_url() -> String {
    std::env::var("ZEROBOX_TEST_URL").unwrap_or_else(|_| "http://localhost:7000".to_string())
}

pub fn test_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("data")
        .join("test")
}

pub fn default_timeout() -> Duration {
    Duration::from_secs(30)
}
