pub mod commands;
pub mod sandboxes;
pub mod snapshots;

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::trace::TraceLayer;

use crate::config::Config;
use crate::manager::SandboxManager;
use crate::snapshot::SnapshotManager;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub sandbox_manager: Arc<SandboxManager>,
    pub snapshot_manager: Arc<SnapshotManager>,
}

pub fn router(state: AppState) -> Router {
    let v1 = Router::new()
        // Sandbox collection: POST to create, GET to list
        .route(
            "/sandboxes",
            post(sandboxes::create_sandbox).get(sandboxes::list_sandboxes),
        )
        // Individual sandbox: GET to fetch, DELETE to destroy
        .route(
            "/sandboxes/:id",
            get(sandboxes::get_sandbox).delete(sandboxes::destroy_sandbox),
        )
        // Sandbox actions
        .route("/sandboxes/:id/stop", post(sandboxes::stop_sandbox))
        // Command routes
        .route(
            "/sandboxes/:id/commands",
            post(commands::run_command),
        )
        .route(
            "/sandboxes/:id/commands/:cmd_id",
            get(commands::get_command),
        )
        .route(
            "/sandboxes/:id/commands/:cmd_id/kill",
            post(commands::kill_command),
        )
        .route(
            "/sandboxes/:id/commands/:cmd_id/logs",
            get(commands::stream_logs),
        )
        // File routes
        .route(
            "/sandboxes/:id/files/write",
            post(sandboxes::write_files),
        )
        .route(
            "/sandboxes/:id/files/read",
            get(sandboxes::read_file),
        )
        .route("/sandboxes/:id/mkdir", post(sandboxes::mkdir))
        // Sandbox snapshot and timeout routes
        .route(
            "/sandboxes/:id/snapshot",
            post(sandboxes::create_snapshot_from_sandbox),
        )
        .route(
            "/sandboxes/:id/extend-timeout",
            post(sandboxes::extend_timeout),
        )
        // Snapshot routes
        .route("/snapshots", get(snapshots::list_snapshots))
        .route(
            "/snapshots/:id",
            get(snapshots::get_snapshot).delete(snapshots::delete_snapshot),
        );

    Router::new()
        .nest("/v1", v1)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}
