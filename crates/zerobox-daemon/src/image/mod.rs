pub mod ext4;
pub mod oci;
pub mod overlay;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::config::ImagesConfig;

pub struct ImageManager {
    config: ImagesConfig,
}

impl ImageManager {
    pub fn new(config: ImagesConfig) -> Self {
        Self { config }
    }

    /// Resolves an image reference to a local rootfs ext4 file path.
    ///
    /// First checks the cache directory for a pre-built image. If not found,
    /// attempts to pull the image from an OCI registry.
    pub async fn resolve_rootfs(&self, image: &str) -> Result<PathBuf> {
        // Check if a pre-built image exists in the cache directory
        let cached = PathBuf::from(&self.config.cache_dir).join(format!("{}.ext4", image));
        if tokio::fs::metadata(&cached).await.is_ok() {
            return Ok(cached);
        }

        // Try to pull the OCI image
        let output_dir = PathBuf::from(&self.config.cache_dir).join(image);
        tokio::fs::create_dir_all(&output_dir).await?;

        let rootfs_path = oci::pull_image(image, &output_dir).await?;
        Ok(rootfs_path)
    }

    /// Creates a copy-on-write overlay of the base rootfs for a specific sandbox,
    /// so each sandbox gets its own writable layer.
    pub async fn create_overlay(
        &self,
        base_rootfs: &Path,
        sandbox_id: &str,
    ) -> Result<PathBuf> {
        overlay::create_overlay(base_rootfs, &self.config.cache_dir, sandbox_id).await
    }
}
