use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_protocol::permissions::{
    FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry, FileSystemSandboxPolicy,
    FileSystemSpecialPath, NetworkSandboxPolicy,
};
use codex_protocol::protocol::SandboxPolicy;
use codex_sandboxing::{
    SandboxCommand, SandboxManager, SandboxTransformRequest, SandboxType, get_platform_sandbox,
};
use codex_utils_absolute_path::AbsolutePathBuf;

/// Run a command inside a cross-platform sandbox.
///
/// Permissions are deny-by-default for writes and network. Reads are allowed
/// everywhere unless restricted with --allow-read=<paths>.
///
/// Deny flags carve out exceptions within allowed paths and always take
/// precedence over allow flags.
///
/// Examples:
///   zerobox-exec -- node -e "console.log('hello')"
///   zerobox-exec --allow-write=. --deny-write=./.git -- node script.js
///   zerobox-exec --allow-read=/tmp --allow-write=/tmp -- node script.js
///   zerobox-exec --allow-net -- curl https://example.com
///   zerobox-exec --allow-all -- bash -c "echo anything goes"
#[derive(Parser, Debug)]
#[command(name = "zerobox-exec", version, about, long_about = None)]
struct Cli {
    /// Restrict readable user data to these paths only (comma-separated).
    /// System libraries and binaries remain accessible for execution.
    /// By default all reads are allowed.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    allow_read: Option<Vec<PathBuf>>,

    /// Block reading from these paths (comma-separated). Takes precedence
    /// over --allow-read.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    deny_read: Option<Vec<PathBuf>>,

    /// Allow writing to these paths (comma-separated).
    /// Without a value, allows writing everywhere.
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    allow_write: Option<Vec<PathBuf>>,

    /// Block writing to these paths (comma-separated). Takes precedence
    /// over --allow-write.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    deny_write: Option<Vec<PathBuf>>,

    /// Allow outbound network access.
    #[arg(long)]
    allow_net: bool,

    /// Grant all permissions (no sandbox). Use with caution.
    #[arg(long, short = 'A')]
    allow_all: bool,

    /// Working directory for the sandboxed command.
    #[arg(long, short = 'C')]
    cwd: Option<PathBuf>,

    /// Disable the sandbox entirely (just run the command).
    #[arg(long)]
    no_sandbox: bool,

    /// The command and arguments to run.
    #[arg(trailing_var_arg = true, required = true)]
    command: Vec<String>,
}

/// Pre-resolved paths from CLI flags.
struct ResolvedPaths {
    readable: Option<Vec<AbsolutePathBuf>>,
    deny_readable: Vec<AbsolutePathBuf>,
    writable: Option<Vec<AbsolutePathBuf>>,
    deny_writable: Vec<AbsolutePathBuf>,
    full_write: bool,
}

fn resolve_path(base: &Path, p: &Path) -> Result<AbsolutePathBuf> {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    };
    AbsolutePathBuf::try_from(abs).context("failed to resolve path")
}

fn resolve_all(base: &Path, paths: &[PathBuf]) -> Result<Vec<AbsolutePathBuf>> {
    paths.iter().map(|p| resolve_path(base, p)).collect()
}

fn resolve_cli_paths(cli: &Cli, cwd: &Path) -> Result<ResolvedPaths> {
    let readable = cli
        .allow_read
        .as_ref()
        .map(|paths| resolve_all(cwd, paths))
        .transpose()?;

    let deny_readable = cli
        .deny_read
        .as_ref()
        .map(|paths| resolve_all(cwd, paths))
        .transpose()?
        .unwrap_or_default();

    let (writable, full_write) = match &cli.allow_write {
        Some(paths) if paths.is_empty() => (None, true),
        Some(paths) => (Some(resolve_all(cwd, paths)?), false),
        None => (None, false),
    };

    let deny_writable = cli
        .deny_write
        .as_ref()
        .map(|paths| resolve_all(cwd, paths))
        .transpose()?
        .unwrap_or_default();

    Ok(ResolvedPaths {
        readable,
        deny_readable,
        writable,
        deny_writable,
        full_write,
    })
}

// ── Policy builders: CLI flags → Codex policy types ──

fn make_root_entry(access: FileSystemAccessMode) -> FileSystemSandboxEntry {
    FileSystemSandboxEntry {
        path: FileSystemPath::Special {
            value: FileSystemSpecialPath::Root,
        },
        access,
    }
}

fn make_path_entries(
    paths: &[AbsolutePathBuf],
    access: FileSystemAccessMode,
) -> Vec<FileSystemSandboxEntry> {
    paths
        .iter()
        .map(|abs| FileSystemSandboxEntry {
            path: FileSystemPath::Path { path: abs.clone() },
            access,
        })
        .collect()
}

fn build_fs_policy(resolved: &ResolvedPaths, allow_all: bool) -> FileSystemSandboxPolicy {
    if allow_all {
        return FileSystemSandboxPolicy::unrestricted();
    }

    let mut entries: Vec<FileSystemSandboxEntry> = Vec::new();

    // Read policy.
    match &resolved.readable {
        Some(paths) => {
            // --allow-read=<paths>: include Minimal platform defaults so
            // binaries, libraries and frameworks are still loadable.
            entries.push(FileSystemSandboxEntry {
                path: FileSystemPath::Special {
                    value: FileSystemSpecialPath::Minimal,
                },
                access: FileSystemAccessMode::Read,
            });
            entries.extend(make_path_entries(paths, FileSystemAccessMode::Read));
        }
        None => {
            entries.push(make_root_entry(FileSystemAccessMode::Read));
        }
    }

    // --deny-read: FileSystemAccessMode::None takes precedence.
    entries.extend(make_path_entries(
        &resolved.deny_readable,
        FileSystemAccessMode::None,
    ));

    // Write policy.
    if resolved.full_write {
        entries.push(make_root_entry(FileSystemAccessMode::Write));
    } else if let Some(paths) = &resolved.writable {
        entries.extend(make_path_entries(paths, FileSystemAccessMode::Write));
    }

    // --deny-write: downgrade to Read (removes write, keeps read).
    entries.extend(make_path_entries(
        &resolved.deny_writable,
        FileSystemAccessMode::Read,
    ));

    FileSystemSandboxPolicy::restricted(entries)
}

fn build_net_policy(allow_all: bool, allow_net: bool) -> NetworkSandboxPolicy {
    if allow_all || allow_net {
        NetworkSandboxPolicy::Enabled
    } else {
        NetworkSandboxPolicy::Restricted
    }
}

fn build_legacy_sandbox_policy(
    resolved: &ResolvedPaths,
    allow_all: bool,
    allow_net: bool,
) -> SandboxPolicy {
    if allow_all || resolved.full_write {
        return SandboxPolicy::DangerFullAccess;
    }

    if let Some(writable_roots) = &resolved.writable {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots.clone(),
            read_only_access: Default::default(),
            network_access: allow_net,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    } else {
        SandboxPolicy::ReadOnly {
            access: Default::default(),
            network_access: allow_net,
        }
    }
}

// ── Execution ──

fn exit_code_from_status(status: std::process::ExitStatus) -> ExitCode {
    if let Some(code) = status.code() {
        return ExitCode::from(code as u8);
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return ExitCode::from((128 + signal) as u8);
        }
    }
    ExitCode::from(1)
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let cwd = match cli.cwd.clone().map_or_else(std::env::current_dir, Ok) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: cannot determine working directory: {e}");
            return ExitCode::from(1);
        }
    };

    let resolved = match resolve_cli_paths(&cli, &cwd) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e:#}");
            return ExitCode::from(1);
        }
    };

    let sandbox_type = if cli.no_sandbox || cli.allow_all {
        SandboxType::None
    } else {
        get_platform_sandbox(false).unwrap_or(SandboxType::None)
    };

    let fs_policy = build_fs_policy(&resolved, cli.allow_all);
    let net_policy = build_net_policy(cli.allow_all, cli.allow_net);
    let legacy_policy = build_legacy_sandbox_policy(&resolved, cli.allow_all, cli.allow_net);

    let program = cli.command[0].clone();
    let args: Vec<String> = cli.command[1..].to_vec();
    let env: HashMap<String, String> = std::env::vars().collect();

    let manager = SandboxManager::new();
    let request = SandboxTransformRequest {
        command: SandboxCommand {
            program,
            args,
            cwd: cwd.clone(),
            env,
            additional_permissions: None,
        },
        policy: &legacy_policy,
        file_system_policy: &fs_policy,
        network_policy: net_policy,
        sandbox: sandbox_type,
        enforce_managed_network: false,
        network: None,
        sandbox_policy_cwd: &cwd,
        #[cfg(target_os = "macos")]
        macos_seatbelt_profile_extensions: None,
        codex_linux_sandbox_exe: None,
        use_legacy_landlock: false,
        windows_sandbox_level: WindowsSandboxLevel::default(),
        windows_sandbox_private_desktop: false,
    };

    let exec_request = match manager.transform(request) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: sandbox transform failed: {e}");
            return ExitCode::from(1);
        }
    };

    let mut cmd = tokio::process::Command::new(&exec_request.command[0]);
    cmd.args(&exec_request.command[1..]);
    cmd.current_dir(&cwd);
    cmd.env_clear();
    for (k, v) in &exec_request.env {
        cmd.env(k, v);
    }

    match cmd.status().await {
        Ok(status) => exit_code_from_status(status),
        Err(e) => {
            eprintln!("error: failed to execute command: {e}");
            ExitCode::from(1)
        }
    }
}
