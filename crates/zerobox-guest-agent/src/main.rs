mod executor;
mod files;
mod health;
mod shell;

use std::collections::HashMap;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Child;
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use zerobox_common::protocol::*;

/// State shared across all handler tasks.
pub struct AgentState {
    pub commands: RwLock<HashMap<String, CommandEntry>>,
    pub start_time: Instant,
    pub next_cmd_id: AtomicU64,
}

/// Tracks a spawned command and its output.
pub struct CommandEntry {
    pub child: Option<Child>,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub running: bool,
}

impl AgentState {
    fn new() -> Self {
        Self {
            commands: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
            next_cmd_id: AtomicU64::new(1),
        }
    }

    /// Generate a unique command ID using the atomic counter.
    pub fn generate_cmd_id(&self) -> String {
        let id = self
            .next_cmd_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("cmd_{id:012x}")
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("zerobox guest agent starting");

    let state = Arc::new(AgentState::new());

    #[cfg(target_os = "linux")]
    {
        run_vsock_listener(state).await?;
    }

    #[cfg(not(target_os = "linux"))]
    {
        // On non-Linux (e.g. macOS dev), listen on a Unix socket for testing
        let sock_path = std::env::var("ZEROBOX_AGENT_SOCK")
            .unwrap_or_else(|_| "/tmp/zerobox-agent.sock".into());
        info!(path = %sock_path, "vsock not available, listening on unix socket (dev mode)");
        let _ = std::fs::remove_file(&sock_path);
        let listener =
            tokio::net::UnixListener::bind(&sock_path).context("failed to bind unix socket")?;
        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    info!("accepted unix connection (dev mode)");
                    let state = Arc::clone(&state);
                    tokio::spawn(async move {
                        let (reader, writer) = tokio::io::split(stream);
                        if let Err(e) = handle_connection(state, reader, writer).await {
                            error!(?e, "connection handler failed");
                        }
                    });
                }
                Err(e) => {
                    error!(?e, "failed to accept connection");
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    Ok(())
}

#[cfg(target_os = "linux")]
async fn run_vsock_listener(state: Arc<AgentState>) -> Result<()> {
    let addr = tokio_vsock::VsockAddr::new(tokio_vsock::VMADDR_CID_ANY, VSOCK_PORT);
    let mut listener =
        tokio_vsock::VsockListener::bind(addr).context("failed to bind vsock listener")?;

    info!(port = VSOCK_PORT, "listening on vsock");

    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!(?addr, "accepted vsock connection");
                let state = Arc::clone(&state);
                tokio::spawn(async move {
                    let (reader, writer) = tokio::io::split(stream);
                    if let Err(e) = handle_connection(state, reader, writer).await {
                        error!(?e, "connection handler failed");
                    }
                });
            }
            Err(e) => {
                error!(?e, "failed to accept vsock connection");
            }
        }
    }
}

async fn handle_connection<R, W>(state: Arc<AgentState>, reader: R, mut writer: W) -> Result<()>
where
    R: tokio::io::AsyncRead + Unpin,
    W: tokio::io::AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let request: RpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                warn!(?e, "invalid JSON-RPC request");
                let resp = RpcResponse {
                    id: 0,
                    result: None,
                    error: Some(RpcError {
                        code: -32700,
                        message: format!("parse error: {e}"),
                    }),
                };
                let mut buf = serde_json::to_vec(&resp)?;
                buf.push(b'\n');
                writer.write_all(&buf).await?;
                continue;
            }
        };

        let request_id = request.id;

        // Special case: shell takes over the connection entirely
        if request.method == METHOD_SHELL {
            // Send success response, then switch to raw byte mode
            let resp = RpcResponse {
                id: request_id,
                result: Some(serde_json::json!({"status": "ok"})),
                error: None,
            };
            let mut buf = serde_json::to_vec(&resp)?;
            buf.push(b'\n');
            writer.write_all(&buf).await?;
            writer.flush().await?;

            // The BufReader may have buffered data — get the inner reader back
            let inner_reader = lines.into_inner().into_inner();

            // Hand off to the shell handler (this blocks until the shell exits)
            if let Err(e) = shell::handle_shell(inner_reader, writer).await {
                tracing::warn!("Shell session error: {}", e);
            }
            return Ok(());
        }

        // Special case: stream_logs returns multiple lines
        if request.method == METHOD_STREAM_LOGS {
            match serde_json::from_value::<StreamLogsParams>(request.params.clone()) {
                Ok(params) => {
                    match executor::handle_stream_logs(Arc::clone(&state), params).await {
                        Ok(entries) => {
                            for entry in entries {
                                let resp = RpcResponse {
                                    id: request_id,
                                    result: Some(entry),
                                    error: None,
                                };
                                let mut buf = serde_json::to_vec(&resp)?;
                                buf.push(b'\n');
                                writer.write_all(&buf).await?;
                            }
                        }
                        Err(e) => {
                            let resp = RpcResponse {
                                id: request_id,
                                result: None,
                                error: Some(RpcError {
                                    code: -32000,
                                    message: format!("{e:#}"),
                                }),
                            };
                            let mut buf = serde_json::to_vec(&resp)?;
                            buf.push(b'\n');
                            writer.write_all(&buf).await?;
                        }
                    }
                }
                Err(e) => {
                    let resp = RpcResponse {
                        id: request_id,
                        result: None,
                        error: Some(RpcError {
                            code: -32602,
                            message: format!("invalid params: {e}"),
                        }),
                    };
                    let mut buf = serde_json::to_vec(&resp)?;
                    buf.push(b'\n');
                    writer.write_all(&buf).await?;
                }
            }
            continue;
        }

        let response = dispatch(Arc::clone(&state), &request).await;

        let resp = match response {
            Ok(value) => RpcResponse {
                id: request_id,
                result: Some(value),
                error: None,
            },
            Err(e) => RpcResponse {
                id: request_id,
                result: None,
                error: Some(RpcError {
                    code: -32000,
                    message: format!("{e:#}"),
                }),
            },
        };

        let mut buf = serde_json::to_vec(&resp)?;
        buf.push(b'\n');
        writer.write_all(&buf).await?;
    }

    info!("connection closed");
    Ok(())
}

async fn dispatch(state: Arc<AgentState>, request: &RpcRequest) -> Result<serde_json::Value> {
    match request.method.as_str() {
        METHOD_EXEC => {
            let params: ExecParams =
                serde_json::from_value(request.params.clone()).context("invalid exec params")?;
            executor::handle_exec(state, params).await
        }
        METHOD_EXEC_DETACHED => {
            let params: ExecParams = serde_json::from_value(request.params.clone())
                .context("invalid exec_detached params")?;
            executor::handle_exec_detached(state, params).await
        }
        METHOD_KILL => {
            let params: KillParams =
                serde_json::from_value(request.params.clone()).context("invalid kill params")?;
            executor::handle_kill(state, params).await
        }
        METHOD_GET_COMMAND => {
            let params: GetCommandParams = serde_json::from_value(request.params.clone())
                .context("invalid get_command params")?;
            executor::handle_get_command(state, params).await
        }
        METHOD_WRITE_FILE => {
            let params: WriteFileParams = serde_json::from_value(request.params.clone())
                .context("invalid write_file params")?;
            files::handle_write_file(state, params).await
        }
        METHOD_READ_FILE => {
            let params: ReadFileParams = serde_json::from_value(request.params.clone())
                .context("invalid read_file params")?;
            files::handle_read_file(state, params).await
        }
        METHOD_MKDIR => {
            let params: MkdirParams =
                serde_json::from_value(request.params.clone()).context("invalid mkdir params")?;
            files::handle_mkdir(state, params).await
        }
        METHOD_HEALTH => health::handle_health(state).await,
        other => anyhow::bail!("unknown method: {other}"),
    }
}
