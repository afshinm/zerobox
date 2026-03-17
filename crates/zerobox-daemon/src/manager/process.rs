use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use tokio::process::{Child, Command};

/// Manages a running Firecracker process and its associated file paths.
pub struct FirecrackerProcess {
    child: Child,
    pub socket_path: PathBuf,
    pub vsock_path: PathBuf,
    pub log_path: PathBuf,
}

impl FirecrackerProcess {
    /// Spawns a new Firecracker process for the given sandbox.
    ///
    /// The process is started with:
    /// - `--api-sock` pointing to `<sandbox_dir>/firecracker.sock`
    /// - Stdout/stderr redirected to log files
    ///
    /// The vsock UDS path is set to `<sandbox_dir>/vsock.sock`.
    pub async fn spawn(
        firecracker_bin: &str,
        _sandbox_id: &str,
        sandbox_dir: &Path,
    ) -> Result<Self> {
        let socket_path = sandbox_dir.join("firecracker.sock");
        let vsock_path = sandbox_dir.join("vsock.sock");
        let log_path = sandbox_dir.join("firecracker.log");

        // Ensure the sandbox directory exists
        tokio::fs::create_dir_all(sandbox_dir).await?;

        // Remove stale socket if it exists
        let _ = tokio::fs::remove_file(&socket_path).await;

        // Firecracker requires the log file to exist before starting
        tokio::fs::write(&log_path, b"").await?;

        let child = Command::new(firecracker_bin)
            .arg("--api-sock")
            .arg(&socket_path)
            .arg("--log-path")
            .arg(&log_path)
            .arg("--level")
            .arg("Info")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| {
                anyhow!(
                    "Failed to spawn Firecracker process at '{}': {}",
                    firecracker_bin,
                    e
                )
            })?;

        Ok(Self {
            child,
            socket_path,
            vsock_path,
            log_path,
        })
    }

    /// Kills the Firecracker process.
    pub async fn kill(&mut self) -> Result<()> {
        self.child
            .kill()
            .await
            .map_err(|e| anyhow!("Failed to kill Firecracker process: {}", e))
    }

    /// Returns the PID of the Firecracker process, if still running.
    pub fn pid(&self) -> Option<u32> {
        self.child.id()
    }
}

/// Polls for a Unix socket file to appear on disk, returning `true` if it
/// appears within `timeout`, `false` otherwise.
pub async fn wait_for_socket(path: &Path, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if tokio::fs::metadata(path).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}
