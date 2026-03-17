use std::sync::Arc;

use anyhow::{bail, Context, Result};
use nix::sys::signal::{self, Signal};
use nix::unistd::Pid;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tracing::{info, warn};

use zerobox_common::protocol::*;

use crate::{AgentState, CommandEntry};

/// Execute a command synchronously, waiting for completion.
pub async fn handle_exec(state: Arc<AgentState>, params: ExecParams) -> Result<serde_json::Value> {
    let cmd_id = state.generate_cmd_id();

    let mut command = build_command(&params);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    info!(cmd_id = %cmd_id, cmd = %params.cmd, "executing command");

    let mut child = command.spawn().context("failed to spawn process")?;
    let pid = child.id();

    // Read stdout and stderr concurrently to avoid pipe buffer deadlocks.
    // If we read them sequentially and the child writes enough to stderr to fill
    // the pipe buffer, the child blocks and stdout never reaches EOF.
    let stdout_fut = async {
        let mut buf = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            stdout
                .read_to_string(&mut buf)
                .await
                .context("failed to read stdout")?;
        }
        Ok::<String, anyhow::Error>(buf)
    };
    let stderr_fut = async {
        let mut buf = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            stderr
                .read_to_string(&mut buf)
                .await
                .context("failed to read stderr")?;
        }
        Ok::<String, anyhow::Error>(buf)
    };

    let (stdout_buf, stderr_buf) = tokio::try_join!(stdout_fut, stderr_fut)?;

    let status = child.wait().await.context("failed to wait for process")?;
    let exit_code = status.code();

    info!(cmd_id = %cmd_id, ?exit_code, "command completed");

    let entry = CommandEntry {
        child: None,
        pid,
        exit_code,
        stdout: stdout_buf.clone(),
        stderr: stderr_buf.clone(),
        running: false,
    };

    state.commands.write().await.insert(cmd_id.clone(), entry);

    let result = ExecResult {
        cmd_id,
        exit_code,
        stdout: stdout_buf,
        stderr: stderr_buf,
    };

    Ok(serde_json::to_value(result)?)
}

/// Execute a command in the background (detached), returning immediately.
pub async fn handle_exec_detached(
    state: Arc<AgentState>,
    params: ExecParams,
) -> Result<serde_json::Value> {
    let cmd_id = state.generate_cmd_id();

    let mut command = build_command(&params);
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());

    info!(cmd_id = %cmd_id, cmd = %params.cmd, "executing detached command");

    let child = command.spawn().context("failed to spawn process")?;
    let pid = child.id();

    let entry = CommandEntry {
        child: Some(child),
        pid,
        exit_code: None,
        stdout: String::new(),
        stderr: String::new(),
        running: true,
    };

    state.commands.write().await.insert(cmd_id.clone(), entry);

    // Spawn a background task to wait for completion and collect output.
    let bg_state = Arc::clone(&state);
    let bg_cmd_id = cmd_id.clone();
    tokio::spawn(async move {
        wait_for_detached(bg_state, bg_cmd_id).await;
    });

    let result = ExecDetachedResult { cmd_id };

    Ok(serde_json::to_value(result)?)
}

/// Background task that waits for a detached process to finish.
async fn wait_for_detached(state: Arc<AgentState>, cmd_id: String) {
    // Take the child out of the entry so we can await it without holding the lock.
    let child = {
        let mut commands = state.commands.write().await;
        commands
            .get_mut(&cmd_id)
            .and_then(|entry| entry.child.take())
    };

    let Some(mut child) = child else {
        warn!(cmd_id = %cmd_id, "detached command has no child process");
        return;
    };

    // Read stdout and stderr concurrently to avoid pipe buffer deadlocks.
    let stdout_fut = async {
        let mut buf = String::new();
        if let Some(mut stdout) = child.stdout.take() {
            let _ = stdout.read_to_string(&mut buf).await;
        }
        buf
    };
    let stderr_fut = async {
        let mut buf = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            let _ = stderr.read_to_string(&mut buf).await;
        }
        buf
    };

    let (stdout_buf, stderr_buf) = tokio::join!(stdout_fut, stderr_fut);

    let status = child.wait().await;
    let exit_code = status.ok().and_then(|s| s.code());

    info!(cmd_id = %cmd_id, ?exit_code, "detached command completed");

    let mut commands = state.commands.write().await;
    if let Some(entry) = commands.get_mut(&cmd_id) {
        entry.exit_code = exit_code;
        entry.stdout = stdout_buf;
        entry.stderr = stderr_buf;
        entry.running = false;
        entry.child = None;
    }
}

/// Kill a running command by its ID.
pub async fn handle_kill(state: Arc<AgentState>, params: KillParams) -> Result<serde_json::Value> {
    let commands = state.commands.read().await;
    let entry = commands.get(&params.cmd_id).context("command not found")?;

    if !entry.running {
        bail!("command {} is not running", params.cmd_id);
    }

    let pid = entry.pid.context("no PID available for command")?;

    let signal = match params.signal.as_deref() {
        Some("SIGKILL") | Some("9") => Signal::SIGKILL,
        Some("SIGINT") | Some("2") => Signal::SIGINT,
        Some("SIGQUIT") | Some("3") => Signal::SIGQUIT,
        Some("SIGHUP") | Some("1") => Signal::SIGHUP,
        // Default to SIGTERM
        None | Some("SIGTERM") | Some("15") => Signal::SIGTERM,
        Some(other) => bail!("unsupported signal: {other}"),
    };

    info!(cmd_id = %params.cmd_id, ?signal, pid, "sending signal to process");

    signal::kill(Pid::from_raw(pid as i32), signal).context("failed to send signal")?;

    // Note: we use a read lock, not write - the background task will update
    // the entry when the process actually exits

    Ok(serde_json::json!({}))
}

/// Get the current status of a command.
pub async fn handle_get_command(
    state: Arc<AgentState>,
    params: GetCommandParams,
) -> Result<serde_json::Value> {
    let commands = state.commands.read().await;
    let entry = commands.get(&params.cmd_id).context("command not found")?;

    let result = CommandStatusResult {
        cmd_id: params.cmd_id,
        exit_code: entry.exit_code,
        running: entry.running,
    };

    Ok(serde_json::to_value(result)?)
}

/// Stream collected logs for a command. Returns all collected stdout/stderr as LogEntry items.
pub async fn handle_stream_logs(
    state: Arc<AgentState>,
    params: StreamLogsParams,
) -> Result<Vec<serde_json::Value>> {
    let commands = state.commands.read().await;
    let entry = commands.get(&params.cmd_id).context("command not found")?;

    let mut entries = Vec::new();

    if !entry.stdout.is_empty() {
        entries.push(serde_json::to_value(LogEntry {
            stream: "stdout".to_string(),
            data: entry.stdout.clone(),
        })?);
    }

    if !entry.stderr.is_empty() {
        entries.push(serde_json::to_value(LogEntry {
            stream: "stderr".to_string(),
            data: entry.stderr.clone(),
        })?);
    }

    Ok(entries)
}

/// Build a `tokio::process::Command` from ExecParams.
fn build_command(params: &ExecParams) -> Command {
    let (program, args) = if params.sudo {
        let mut sudo_args = vec![params.cmd.clone()];
        sudo_args.extend(params.args.iter().cloned());
        ("sudo".to_string(), sudo_args)
    } else {
        (params.cmd.clone(), params.args.clone())
    };

    let mut command = Command::new(&program);
    command.args(&args);

    if let Some(ref cwd) = params.cwd {
        command.current_dir(cwd);
    }

    if let Some(ref env) = params.env {
        for (key, value) in env {
            command.env(key, value);
        }
    }

    command
}
