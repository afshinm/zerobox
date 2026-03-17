use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use zerobox_common::types::*;

use super::sandboxes::ApiError;
use super::AppState;

pub async fn list_snapshots(
    State(state): State<AppState>,
) -> Result<Json<Vec<SnapshotInfo>>, ApiError> {
    let list = state.snapshot_manager.list().await?;
    Ok(Json(list))
}

pub async fn get_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SnapshotInfo>, ApiError> {
    let info = state.snapshot_manager.get(&id).await?;
    Ok(Json(info))
}

pub async fn delete_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.snapshot_manager.delete(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}
