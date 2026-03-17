use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use tokio::fs;
use tracing::info;

use zerobox_common::protocol::*;

use crate::AgentState;

/// Write a file to the guest filesystem.
pub async fn handle_write_file(
    _state: Arc<AgentState>,
    params: WriteFileParams,
) -> Result<serde_json::Value> {
    let content = BASE64
        .decode(&params.content)
        .context("failed to decode base64 content")?;

    let path = Path::new(&params.path);

    // Create parent directories if they don't exist.
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("failed to create parent directories")?;
    }

    fs::write(path, &content)
        .await
        .with_context(|| format!("failed to write file: {}", params.path))?;

    info!(path = %params.path, bytes = content.len(), "wrote file");

    Ok(serde_json::json!({}))
}

/// Read a file from the guest filesystem.
pub async fn handle_read_file(
    _state: Arc<AgentState>,
    params: ReadFileParams,
) -> Result<serde_json::Value> {
    let path = Path::new(&params.path);

    if tokio::fs::metadata(path).await.is_err() {
        let result = ReadFileResult {
            content: String::new(),
            exists: false,
        };
        return Ok(serde_json::to_value(result)?);
    }

    let content = fs::read(path)
        .await
        .with_context(|| format!("failed to read file: {}", params.path))?;

    let encoded = BASE64.encode(&content);

    info!(path = %params.path, bytes = content.len(), "read file");

    let result = ReadFileResult {
        content: encoded,
        exists: true,
    };

    Ok(serde_json::to_value(result)?)
}

/// Create a directory (recursively) on the guest filesystem.
pub async fn handle_mkdir(
    _state: Arc<AgentState>,
    params: MkdirParams,
) -> Result<serde_json::Value> {
    fs::create_dir_all(&params.path)
        .await
        .with_context(|| format!("failed to create directory: {}", params.path))?;

    info!(path = %params.path, "created directory");

    Ok(serde_json::json!({}))
}
