use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// Returns the directory path for a given snapshot under the base storage directory.
pub fn snapshot_dir(base: &str, snapshot_id: &str) -> PathBuf {
    PathBuf::from(base).join(snapshot_id)
}

/// Saves snapshot files (VM state, memory file, rootfs) into the snapshot directory
/// by copying them from their original locations.
pub async fn save_snapshot_files(
    snapshot_dir: &Path,
    vm_state: &Path,
    mem_file: &Path,
    rootfs: &Path,
) -> Result<()> {
    tokio::fs::create_dir_all(snapshot_dir).await?;

    tokio::fs::copy(vm_state, snapshot_dir.join("vm_state.bin"))
        .await
        .map_err(|e| anyhow!("Failed to copy VM state: {}", e))?;

    tokio::fs::copy(mem_file, snapshot_dir.join("mem_file.bin"))
        .await
        .map_err(|e| anyhow!("Failed to copy memory file: {}", e))?;

    tokio::fs::copy(rootfs, snapshot_dir.join("rootfs.ext4"))
        .await
        .map_err(|e| anyhow!("Failed to copy rootfs: {}", e))?;

    Ok(())
}

/// Loads snapshot file paths from the snapshot directory.
/// Returns (vm_state_path, mem_file_path, rootfs_path).
/// Uses async filesystem checks to avoid blocking the executor.
pub async fn load_snapshot_files(snapshot_dir: &Path) -> Result<(PathBuf, PathBuf, PathBuf)> {
    let vm_state = snapshot_dir.join("vm_state.bin");
    let mem_file = snapshot_dir.join("mem_file.bin");
    let rootfs = snapshot_dir.join("rootfs.ext4");

    if tokio::fs::metadata(&vm_state).await.is_err() {
        return Err(anyhow!(
            "VM state file not found at {:?}",
            vm_state
        ));
    }
    if tokio::fs::metadata(&mem_file).await.is_err() {
        return Err(anyhow!(
            "Memory file not found at {:?}",
            mem_file
        ));
    }
    if tokio::fs::metadata(&rootfs).await.is_err() {
        return Err(anyhow!("Rootfs not found at {:?}", rootfs));
    }

    Ok((vm_state, mem_file, rootfs))
}
