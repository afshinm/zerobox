use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{anyhow, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use zerobox_common::protocol::*;

/// Client for communicating with the guest agent inside a Firecracker VM
/// via the Firecracker vsock Unix domain socket proxy.
///
/// The connection protocol:
/// 1. Connect to the Unix socket at `socket_path`
/// 2. Send "CONNECT <port>\n" (port 52 is the guest agent port)
/// 3. Read "OK <port>\n" response
/// 4. The stream is now connected to the guest agent -- send/receive
///    JSON-RPC messages (one JSON object per line)
pub struct GuestClient {
    socket_path: PathBuf,
    next_id: AtomicU64,
}

impl GuestClient {
    pub fn new(socket_path: impl AsRef<Path>) -> Self {
        Self {
            socket_path: socket_path.as_ref().to_path_buf(),
            next_id: AtomicU64::new(1),
        }
    }

    /// Low-level RPC call: connects to the vsock UDS, performs the CONNECT
    /// handshake, sends a JSON-RPC request, and reads the JSON-RPC response.
    async fn call(&self, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
        let stream = UnixStream::connect(&self.socket_path).await.map_err(|e| {
            anyhow!(
                "Failed to connect to guest agent socket at {:?}: {}",
                self.socket_path,
                e
            )
        })?;

        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);

        // Step 1: Send CONNECT handshake
        let connect_msg = format!("CONNECT {}\n", VSOCK_PORT);
        writer.write_all(connect_msg.as_bytes()).await?;

        // Step 2: Read OK response
        let mut ok_line = String::new();
        reader.read_line(&mut ok_line).await?;
        if !ok_line.starts_with("OK") {
            return Err(anyhow!(
                "Vsock CONNECT handshake failed. Expected 'OK ...', got: '{}'",
                ok_line.trim()
            ));
        }

        // Step 3: Send JSON-RPC request
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = RpcRequest {
            id,
            method: method.to_string(),
            params,
        };
        let mut request_json = serde_json::to_string(&request)?;
        request_json.push('\n');
        writer.write_all(request_json.as_bytes()).await?;

        // Step 4: Read JSON-RPC response
        let mut response_line = String::new();
        reader.read_line(&mut response_line).await?;

        let response: RpcResponse = serde_json::from_str(&response_line).map_err(|e| {
            anyhow!(
                "Failed to parse guest agent response: {} (raw: '{}')",
                e,
                response_line.trim()
            )
        })?;

        if let Some(err) = response.error {
            return Err(anyhow!(
                "Guest agent RPC error (code {}): {}",
                err.code,
                err.message
            ));
        }

        response
            .result
            .ok_or_else(|| anyhow!("Guest agent returned empty result for method '{}'", method))
    }

    /// Executes a command synchronously in the guest (waits for completion).
    pub async fn exec(&self, params: ExecParams) -> Result<ExecResult> {
        let value = serde_json::to_value(&params)?;
        let result = self.call(METHOD_EXEC, value).await?;
        let exec_result: ExecResult = serde_json::from_value(result)?;
        Ok(exec_result)
    }

    /// Executes a command in detached mode (returns immediately with a command ID).
    pub async fn exec_detached(&self, params: ExecParams) -> Result<ExecDetachedResult> {
        let value = serde_json::to_value(&params)?;
        let result = self.call(METHOD_EXEC_DETACHED, value).await?;
        let detached_result: ExecDetachedResult = serde_json::from_value(result)?;
        Ok(detached_result)
    }

    /// Sends a kill signal to a running command in the guest.
    pub async fn kill(&self, params: KillParams) -> Result<()> {
        let value = serde_json::to_value(&params)?;
        let _ = self.call(METHOD_KILL, value).await?;
        Ok(())
    }

    /// Gets the status of a command running in the guest.
    pub async fn get_command(&self, cmd_id: &str) -> Result<CommandStatusResult> {
        let params = GetCommandParams {
            cmd_id: cmd_id.to_string(),
        };
        let value = serde_json::to_value(&params)?;
        let result = self.call(METHOD_GET_COMMAND, value).await?;
        let status: CommandStatusResult = serde_json::from_value(result)?;
        Ok(status)
    }

    /// Writes a file to the guest filesystem.
    pub async fn write_file(&self, params: WriteFileParams) -> Result<()> {
        let value = serde_json::to_value(&params)?;
        let _ = self.call(METHOD_WRITE_FILE, value).await?;
        Ok(())
    }

    /// Reads a file from the guest filesystem.
    pub async fn read_file(&self, path: &str) -> Result<ReadFileResult> {
        let params = ReadFileParams {
            path: path.to_string(),
        };
        let value = serde_json::to_value(&params)?;
        let result = self.call(METHOD_READ_FILE, value).await?;
        let read_result: ReadFileResult = serde_json::from_value(result)?;
        Ok(read_result)
    }

    /// Creates a directory in the guest filesystem.
    pub async fn mkdir(&self, path: &str) -> Result<()> {
        let params = MkdirParams {
            path: path.to_string(),
        };
        let value = serde_json::to_value(&params)?;
        let _ = self.call(METHOD_MKDIR, value).await?;
        Ok(())
    }

    /// Checks the health of the guest agent.
    pub async fn health(&self) -> Result<HealthResult> {
        let result = self.call(METHOD_HEALTH, serde_json::json!({})).await?;
        let health: HealthResult = serde_json::from_value(result)?;
        Ok(health)
    }
}
