use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type SandboxId = String;
pub type CommandId = String;
pub type SnapshotId = String;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SandboxStatus {
    Pending,
    Running,
    Stopping,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceType {
    Git,
    Snapshot,
    Image,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxSource {
    #[serde(rename = "type")]
    pub source_type: SourceType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Resources {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vcpus: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_mib: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disk_mib: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OutboundPolicy {
    Allow,
    Deny,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkPolicy {
    pub outbound: OutboundPolicy,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSandboxRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SandboxSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resources: Option<Resources>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ports: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network_policy: Option<NetworkPolicy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SandboxInfo {
    pub sandbox_id: SandboxId,
    pub status: SandboxStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip: Option<String>,
    pub created_at: String,
    pub timeout: u64,
    pub ports: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunCommandRequest {
    pub cmd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sudo: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detached: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandInfo {
    pub cmd_id: CommandId,
    pub exit_code: Option<i32>,
    pub started_at: u64,
    pub cwd: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteFilesRequest {
    pub files: Vec<FileWrite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWrite {
    pub path: String,
    pub content: String, // base64-encoded
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MkdirRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtendTimeoutRequest {
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SnapshotStatus {
    Created,
    Deleted,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub snapshot_id: SnapshotId,
    pub source_sandbox_id: SandboxId,
    pub status: SnapshotStatus,
    pub created_at: String,
}
