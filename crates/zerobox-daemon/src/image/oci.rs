use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

/// Pulls an OCI container image and extracts it into the output directory.
///
/// This is a placeholder implementation. A full implementation would use
/// skopeo/umoci or a Rust-native OCI image unpacker to:
/// 1. Pull the image manifest
/// 2. Download and extract layers
/// 3. Create a rootfs directory from the extracted layers
/// 4. Convert the rootfs to an ext4 image
pub async fn pull_image(image_ref: &str, output_dir: &Path) -> Result<PathBuf> {
    // TODO: Implement OCI image pulling
    // For now, return an error indicating the image is not available
    Err(anyhow!(
        "OCI image pulling not yet implemented. Cannot pull '{}' to {:?}",
        image_ref,
        output_dir
    ))
}
