use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// vsock port the guest agent listens on
pub const VSOCK_PORT: u32 = 52;

/// Guest CID (Firecracker always uses CID 3 for the guest)
pub const VSOCK_GUEST_CID: u32 = 3;

// JSON-RPC method names
pub const METHOD_EXEC: &str = "exec";
pub const METHOD_EXEC_DETACHED: &str = "exec_detached";
pub const METHOD_KILL: &str = "kill";
pub const METHOD_GET_COMMAND: &str = "get_command";
pub const METHOD_STREAM_LOGS: &str = "stream_logs";
pub const METHOD_WRITE_FILE: &str = "write_file";
pub const METHOD_READ_FILE: &str = "read_file";
pub const METHOD_MKDIR: &str = "mkdir";
pub const METHOD_HEALTH: &str = "health";
pub const METHOD_SHELL: &str = "shell";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub id: u64,
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcResponse {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

// -- RPC parameter and result types --

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecParams {
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub sudo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecResult {
    pub cmd_id: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecDetachedResult {
    pub cmd_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KillParams {
    pub cmd_id: String,
    pub signal: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetCommandParams {
    pub cmd_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandStatusResult {
    pub cmd_id: String,
    pub exit_code: Option<i32>,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamLogsParams {
    pub cmd_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub stream: String, // "stdout" or "stderr"
    pub data: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileParams {
    pub path: String,
    pub content: String, // base64-encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileParams {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadFileResult {
    pub content: String, // base64-encoded
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkdirParams {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResult {
    pub ok: bool,
    pub uptime_secs: u64,
}
