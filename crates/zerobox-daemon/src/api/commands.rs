use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

use zerobox_common::types::*;

use super::sandboxes::ApiError;
use super::AppState;

pub async fn run_command(
    State(state): State<AppState>,
    Path(sandbox_id): Path<String>,
    Json(req): Json<RunCommandRequest>,
) -> Result<(StatusCode, Json<CommandInfo>), ApiError> {
    let info = state.sandbox_manager.run_command(&sandbox_id, req).await?;
    Ok((StatusCode::CREATED, Json(info)))
}

pub async fn get_command(
    State(state): State<AppState>,
    Path((sandbox_id, cmd_id)): Path<(String, String)>,
) -> Result<Json<CommandInfo>, ApiError> {
    let info = state.sandbox_manager.get_command(&sandbox_id, &cmd_id).await?;
    Ok(Json(info))
}

pub async fn kill_command(
    State(state): State<AppState>,
    Path((sandbox_id, cmd_id)): Path<(String, String)>,
) -> Result<StatusCode, ApiError> {
    state.sandbox_manager.kill_command(&sandbox_id, &cmd_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn stream_logs(
    State(_state): State<AppState>,
    Path((_sandbox_id, _cmd_id)): Path<(String, String)>,
) -> Result<Response, ApiError> {
    // SSE streaming placeholder
    // In a full implementation, this would open a Server-Sent Events stream
    // that forwards stdout/stderr from the guest agent in real-time.
    let body = serde_json::json!({
        "error": "SSE log streaming not yet implemented"
    });
    Ok((StatusCode::NOT_IMPLEMENTED, Json(body)).into_response())
}
