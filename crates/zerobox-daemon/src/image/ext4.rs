use std::path::Path;

use anyhow::{anyhow, Result};
use tokio::process::Command;

/// Creates an ext4 filesystem image from a directory.
///
/// Runs `mkfs.ext4` to create a blank ext4 image of the specified size,
/// then mounts it and copies the contents of `dir` into it.
pub async fn create_ext4(dir: &Path, output: &Path, size_mib: u32) -> Result<()> {
    // Create a blank file of the specified size
    let status = Command::new("dd")
        .args([
            "if=/dev/zero",
            &format!("of={}", output.display()),
            "bs=1M",
            &format!("count={}", size_mib),
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to create blank image file: {}", e))?;

    if !status.success() {
        return Err(anyhow!("dd failed to create image file at {:?}", output));
    }

    // Format as ext4
    let status = Command::new("mkfs.ext4")
        .args(["-F", &output.display().to_string()])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run mkfs.ext4: {}", e))?;

    if !status.success() {
        return Err(anyhow!("mkfs.ext4 failed for {:?}", output));
    }

    // Mount, copy contents, unmount
    let mount_point = output.parent().unwrap_or(Path::new("/tmp")).join("mnt_tmp");
    tokio::fs::create_dir_all(&mount_point).await?;

    let status = Command::new("mount")
        .args([
            "-o",
            "loop",
            &output.display().to_string(),
            &mount_point.display().to_string(),
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to mount ext4 image: {}", e))?;

    if !status.success() {
        return Err(anyhow!("Failed to mount image at {:?}", mount_point));
    }

    // Copy contents
    let status = Command::new("cp")
        .args([
            "-a",
            &format!("{}/.", dir.display()),
            &mount_point.display().to_string(),
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to copy contents to ext4 image: {}", e))?;

    // Always try to unmount, even if copy failed
    let umount_status = Command::new("umount")
        .arg(mount_point.display().to_string())
        .status()
        .await;

    if !status.success() {
        return Err(anyhow!("Failed to copy directory contents into ext4 image"));
    }

    if let Ok(s) = umount_status {
        if !s.success() {
            return Err(anyhow!(
                "Failed to unmount ext4 image from {:?}",
                mount_point
            ));
        }
    }

    // Clean up mount point
    let _ = tokio::fs::remove_dir(&mount_point).await;

    Ok(())
}
