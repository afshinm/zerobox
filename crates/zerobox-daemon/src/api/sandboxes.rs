use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine as _;
use serde::Deserialize;

use zerobox_common::types::*;

use super::AppState;
use crate::manager::SandboxError;
use crate::snapshot::SnapshotError;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::NotFound(msg) => (StatusCode::NOT_FOUND, msg),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        let body = serde_json::json!({ "error": message });
        (status, Json(body)).into_response()
    }
}

impl From<SandboxError> for ApiError {
    fn from(err: SandboxError) -> Self {
        match err {
            SandboxError::NotFound(msg) => ApiError::NotFound(msg),
            SandboxError::InvalidState(msg) => ApiError::BadRequest(msg),
            SandboxError::Internal(err) => ApiError::Internal(err.to_string()),
        }
    }
}

impl From<SnapshotError> for ApiError {
    fn from(err: SnapshotError) -> Self {
        match err {
            SnapshotError::NotFound(msg) => ApiError::NotFound(msg),
            SnapshotError::Internal(err) => ApiError::Internal(err.to_string()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::Internal(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// Query types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ReadFileQuery {
    pub path: String,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

pub async fn create_sandbox(
    State(state): State<AppState>,
    Json(req): Json<CreateSandboxRequest>,
) -> Result<(StatusCode, Json<SandboxInfo>), ApiError> {
    let info = state.sandbox_manager.create(req).await?;
    Ok((StatusCode::CREATED, Json(info)))
}

pub async fn list_sandboxes(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let list = state.sandbox_manager.list().await?;
    Ok(Json(serde_json::json!({ "sandboxes": list })))
}

pub async fn get_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SandboxInfo>, ApiError> {
    let info = state.sandbox_manager.get(&id).await?;
    Ok(Json(info))
}

pub async fn stop_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<SandboxInfo>, ApiError> {
    let info = state.sandbox_manager.stop(&id).await?;
    Ok(Json(info))
}

pub async fn destroy_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    state.sandbox_manager.destroy(&id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn write_files(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<WriteFilesRequest>,
) -> Result<StatusCode, ApiError> {
    state.sandbox_manager.write_files(&id, req).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn read_file(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ReadFileQuery>,
) -> Result<Response, ApiError> {
    let data = state.sandbox_manager.read_file(&id, &query.path).await?;
    match data {
        Some(bytes) => {
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
            let body = serde_json::json!({ "content": encoded });
            Ok(Json(body).into_response())
        }
        None => Err(ApiError::NotFound(format!(
            "File not found: {}",
            query.path
        ))),
    }
}

pub async fn mkdir(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<MkdirRequest>,
) -> Result<StatusCode, ApiError> {
    state.sandbox_manager.mkdir(&id, &req.path).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn create_snapshot_from_sandbox(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<SnapshotInfo>), ApiError> {
    let info = state
        .sandbox_manager
        .create_snapshot(&id, &state.snapshot_manager)
        .await?;
    Ok((StatusCode::CREATED, Json(info)))
}

pub async fn extend_timeout(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ExtendTimeoutRequest>,
) -> Result<StatusCode, ApiError> {
    state
        .sandbox_manager
        .extend_timeout(&id, req.duration_ms)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
