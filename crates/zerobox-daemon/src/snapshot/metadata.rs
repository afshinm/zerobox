use std::path::Path;

use anyhow::Result;
use zerobox_common::types::SnapshotInfo;

/// Generates a snapshot ID in the form "snap_" + first 12 hex chars of a UUID v4.
pub fn generate_snapshot_id() -> String {
    let id = uuid::Uuid::new_v4();
    let hex = id.as_simple().to_string();
    format!("snap_{}", &hex[..12])
}

/// Saves snapshot metadata as JSON to the given path.
pub async fn save_metadata(path: &Path, info: &SnapshotInfo) -> Result<()> {
    let json = serde_json::to_string_pretty(info)?;
    tokio::fs::write(path, json).await?;
    Ok(())
}

/// Loads snapshot metadata from a JSON file at the given path.
pub async fn load_metadata(path: &Path) -> Result<SnapshotInfo> {
    let data = tokio::fs::read_to_string(path).await?;
    let info: SnapshotInfo = serde_json::from_str(&data)?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_snapshot_id() {
        let id = generate_snapshot_id();
        assert!(id.starts_with("snap_"));
        assert_eq!(id.len(), 17); // "snap_" (5) + 12 hex chars
    }
}
