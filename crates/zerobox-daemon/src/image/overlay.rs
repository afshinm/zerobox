use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use tokio::process::Command;

/// Creates an OverlayFS mount with the given base rootfs as the lower (read-only) layer
/// and a new upper layer for the sandbox's writable changes.
///
/// Directory layout under `work_dir`:
///   overlays/<sandbox_id>/upper/   - writable upper layer
///   overlays/<sandbox_id>/work/    - OverlayFS workdir
///   overlays/<sandbox_id>/merged/  - the merged mount point
///
/// Returns the path to the merged directory.
pub async fn create_overlay(
    base: &Path,
    work_dir: &str,
    sandbox_id: &str,
) -> Result<PathBuf> {
    let overlay_base = PathBuf::from(work_dir).join("overlays").join(sandbox_id);
    let upper = overlay_base.join("upper");
    let work = overlay_base.join("work");
    let merged = overlay_base.join("merged");

    tokio::fs::create_dir_all(&upper).await?;
    tokio::fs::create_dir_all(&work).await?;
    tokio::fs::create_dir_all(&merged).await?;

    let mount_opts = format!(
        "lowerdir={},upperdir={},workdir={}",
        base.display(),
        upper.display(),
        work.display()
    );

    let status = Command::new("mount")
        .args([
            "-t",
            "overlay",
            "overlay",
            "-o",
            &mount_opts,
            &merged.display().to_string(),
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to mount overlayfs for sandbox '{}': {}", sandbox_id, e))?;

    if !status.success() {
        return Err(anyhow!(
            "Failed to mount overlayfs for sandbox '{}'",
            sandbox_id
        ));
    }

    Ok(merged)
}

/// Cleans up an OverlayFS mount for a sandbox by unmounting and removing the
/// overlay directories.
pub async fn cleanup_overlay(sandbox_id: &str, work_dir: &str) -> Result<()> {
    let overlay_base = PathBuf::from(work_dir).join("overlays").join(sandbox_id);
    let merged = overlay_base.join("merged");

    // Unmount the overlay
    let status = Command::new("umount")
        .arg(merged.display().to_string())
        .status()
        .await
        .map_err(|e| anyhow!("Failed to unmount overlay for sandbox '{}': {}", sandbox_id, e))?;

    if !status.success() {
        return Err(anyhow!(
            "Failed to unmount overlay for sandbox '{}'",
            sandbox_id
        ));
    }

    // Remove overlay directories
    tokio::fs::remove_dir_all(&overlay_base).await.map_err(|e| {
        anyhow!(
            "Failed to remove overlay directories for sandbox '{}': {}",
            sandbox_id,
            e
        )
    })?;

    Ok(())
}
