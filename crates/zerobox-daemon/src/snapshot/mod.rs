pub mod metadata;
pub mod store;

use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;
use tokio::sync::RwLock;

use crate::config::Config;
use zerobox_common::types::*;

#[derive(Debug, Error)]
pub enum SnapshotError {
    #[error("Snapshot not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}

pub struct SnapshotManager {
    snapshots: RwLock<HashMap<String, SnapshotInfo>>,
    config: Arc<Config>,
}

impl SnapshotManager {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            snapshots: RwLock::new(HashMap::new()),
            config,
        }
    }

    /// Creates a snapshot from a running sandbox.
    ///
    /// In a full implementation, this would:
    /// 1. Pause the Firecracker VM
    /// 2. Request a snapshot via the Firecracker API
    /// 3. Copy snapshot files to the snapshot storage directory
    /// 4. Store metadata
    /// 5. Stop the original Firecracker process
    pub async fn create(&self, sandbox_id: &str) -> Result<SnapshotInfo, SnapshotError> {
        let snapshot_id = metadata::generate_snapshot_id();
        let now = chrono::Utc::now().to_rfc3339();

        let snapshot_dir = store::snapshot_dir(&self.config.snapshots.storage_dir, &snapshot_id);
        tokio::fs::create_dir_all(&snapshot_dir)
            .await
            .map_err(|e| SnapshotError::Internal(e.into()))?;

        let info = SnapshotInfo {
            snapshot_id: snapshot_id.clone(),
            source_sandbox_id: sandbox_id.to_string(),
            status: SnapshotStatus::Created,
            created_at: now,
        };

        // Save metadata to disk
        let metadata_path = snapshot_dir.join("metadata.json");
        metadata::save_metadata(&metadata_path, &info).await?;

        // Store in-memory
        let mut snapshots = self.snapshots.write().await;
        snapshots.insert(snapshot_id, info.clone());

        Ok(info)
    }

    /// Lists all known snapshots.
    pub async fn list(&self) -> Result<Vec<SnapshotInfo>, SnapshotError> {
        let snapshots = self.snapshots.read().await;
        let list: Vec<SnapshotInfo> = snapshots.values().cloned().collect();
        Ok(list)
    }

    /// Gets a snapshot by ID.
    pub async fn get(&self, snapshot_id: &str) -> Result<SnapshotInfo, SnapshotError> {
        let snapshots = self.snapshots.read().await;
        let info = snapshots
            .get(snapshot_id)
            .ok_or_else(|| SnapshotError::NotFound(snapshot_id.to_string()))?;
        Ok(info.clone())
    }

    /// Deletes a snapshot by ID, removing both in-memory state and on-disk files.
    pub async fn delete(&self, snapshot_id: &str) -> Result<(), SnapshotError> {
        let snapshot_dir = {
            let mut snapshots = self.snapshots.write().await;
            if snapshots.remove(snapshot_id).is_none() {
                return Err(SnapshotError::NotFound(snapshot_id.to_string()));
            }
            store::snapshot_dir(&self.config.snapshots.storage_dir, snapshot_id)
        }; // write lock dropped before async I/O

        // Clean up snapshot directory on disk (ignore NotFound — may already be gone)
        match tokio::fs::remove_dir_all(&snapshot_dir).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(SnapshotError::Internal(e.into())),
        }
    }
}
