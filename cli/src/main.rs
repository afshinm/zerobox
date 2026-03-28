use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use codex_network_proxy::{
    ConfigReloader, ConfigState, NetworkProxy, NetworkProxyState, build_config_state,
};
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
///   zerobox-exec --allow-net=example.com,api.example.com -- node script.js
///   zerobox-exec --allow-net --deny-net=evil.com -- node script.js
///   zerobox-exec --allow-all -- bash -c "echo anything goes"
#[derive(Parser, Debug)]
#[command(name = "zerobox", version, about, long_about = None)]
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

    /// Allow outbound network access. Without a value, allows all domains.
    /// With values, restricts to specific domains (comma-separated).
    /// Examples: --allow-net, --allow-net=example.com,api.example.com
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    allow_net: Option<Vec<String>>,

    /// Block network access to these domains (comma-separated).
    /// Takes precedence over --allow-net.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    deny_net: Option<Vec<String>>,

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

    match &resolved.readable {
        Some(paths) => {
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

    entries.extend(make_path_entries(
        &resolved.deny_readable,
        FileSystemAccessMode::None,
    ));

    if resolved.full_write {
        entries.push(make_root_entry(FileSystemAccessMode::Write));
    } else if let Some(paths) = &resolved.writable {
        entries.extend(make_path_entries(paths, FileSystemAccessMode::Write));
    }

    entries.extend(make_path_entries(
        &resolved.deny_writable,
        FileSystemAccessMode::Read,
    ));

    FileSystemSandboxPolicy::restricted(entries)
}

fn net_is_enabled(cli: &Cli) -> bool {
    cli.allow_all || cli.allow_net.is_some()
}

fn build_net_policy(cli: &Cli) -> NetworkSandboxPolicy {
    if net_is_enabled(cli) {
        NetworkSandboxPolicy::Enabled
    } else {
        NetworkSandboxPolicy::Restricted
    }
}

fn build_legacy_sandbox_policy(resolved: &ResolvedPaths, cli: &Cli) -> SandboxPolicy {
    if cli.allow_all || resolved.full_write {
        return SandboxPolicy::DangerFullAccess;
    }

    let network_access = net_is_enabled(cli);

    if let Some(writable_roots) = &resolved.writable {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: writable_roots.clone(),
            read_only_access: Default::default(),
            network_access,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    } else {
        SandboxPolicy::ReadOnly {
            access: Default::default(),
            network_access,
        }
    }
}

// ── Network proxy: domain-level filtering via Codex network-proxy ──

/// A ConfigReloader that never reloads (static config for CLI use).
struct StaticReloader;

#[async_trait::async_trait]
impl ConfigReloader for StaticReloader {
    fn source_label(&self) -> String {
        "zerobox-exec static config".to_string()
    }

    async fn maybe_reload(&self) -> anyhow::Result<Option<ConfigState>> {
        Ok(None)
    }

    async fn reload_now(&self) -> anyhow::Result<ConfigState> {
        Err(anyhow::anyhow!("static config does not support reload"))
    }
}

/// Build a NetworkProxy when --allow-net has domain filters or --deny-net is used.
async fn build_network_proxy(cli: &Cli) -> Result<Option<NetworkProxy>> {
    let Some(allow_domains) = &cli.allow_net else {
        return Ok(None); // Network not enabled.
    };

    let has_filters = !allow_domains.is_empty() || cli.deny_net.is_some();
    if !has_filters {
        return Ok(None); // Full network, no filtering needed.
    }

    use codex_network_proxy::NetworkProxyConfig;
    let mut config = NetworkProxyConfig::default();
    config.network.enabled = true;

    if allow_domains.is_empty() {
        // Bare --allow-net with --deny-net: allow everything except denied.
        config.network.allowed_domains = vec!["*".to_string()];
    } else {
        config.network.allowed_domains = allow_domains.clone();
    }
    if let Some(deny) = &cli.deny_net {
        config.network.denied_domains = deny.clone();
    }

    let state = build_config_state(
        config,
        codex_network_proxy::NetworkProxyConstraints::default(),
    )?;

    let proxy_state = Arc::new(NetworkProxyState::with_reloader(
        state,
        Arc::new(StaticReloader),
    ));

    let proxy = NetworkProxy::builder()
        .state(proxy_state)
        .managed_by_codex(true)
        .build()
        .await?;

    Ok(Some(proxy))
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
    // Arg0 dispatch: when invoked as "codex-linux-sandbox" (e.g. by bubblewrap
    // re-exec), run the Linux sandbox helper instead of the CLI. This makes
    // zerobox a single binary that doubles as the sandbox helper on Linux.
    #[cfg(target_os = "linux")]
    {
        use codex_sandboxing::landlock::CODEX_LINUX_SANDBOX_ARG0;
        let exe_name = std::env::args_os()
            .next()
            .as_ref()
            .and_then(|s| Path::new(s).file_name().map(|f| f.to_os_string()));
        if exe_name.as_deref() == Some(std::ffi::OsStr::new(CODEX_LINUX_SANDBOX_ARG0)) {
            codex_linux_sandbox::run_main(); // never returns
        }
    }

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
    let net_policy = build_net_policy(&cli);
    let legacy_policy = build_legacy_sandbox_policy(&resolved, &cli);

    let proxy = match build_network_proxy(&cli).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to build network proxy: {e:#}");
            return ExitCode::from(1);
        }
    };

    // Start the proxy listeners (HTTP + SOCKS5). The handle keeps them alive
    // until dropped. Must be held for the lifetime of the sandboxed process.
    let _proxy_handle = if let Some(ref proxy) = proxy {
        match proxy.run().await {
            Ok(handle) => Some(handle),
            Err(e) => {
                eprintln!("error: failed to start network proxy: {e:#}");
                return ExitCode::from(1);
            }
        }
    } else {
        None
    };

    // On Linux, the sandbox helper is this same binary (arg0 dispatch).
    // Pass our own exe path; bubblewrap will re-invoke us with
    // argv[0] = "codex-linux-sandbox" which triggers the dispatch above.
    let linux_sandbox_exe: Option<PathBuf> = if cfg!(target_os = "linux") {
        std::env::current_exe().ok()
    } else {
        None
    };

    let manager = SandboxManager::new();
    let request = SandboxTransformRequest {
        command: SandboxCommand {
            program: cli.command[0].clone(),
            args: cli.command[1..].to_vec(),
            cwd: cwd.clone(),
            env: std::env::vars().collect(),
            additional_permissions: None,
        },
        policy: &legacy_policy,
        file_system_policy: &fs_policy,
        network_policy: net_policy,
        sandbox: sandbox_type,
        enforce_managed_network: proxy.is_some(),
        network: proxy.as_ref(),
        sandbox_policy_cwd: &cwd,
        #[cfg(target_os = "macos")]
        macos_seatbelt_profile_extensions: None,
        codex_linux_sandbox_exe: linux_sandbox_exe.as_ref(),
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

    // On Unix, the sandbox transform may set arg0 (e.g. "codex-linux-sandbox")
    // so our arg0 dispatch triggers when bubblewrap re-execs us.
    #[cfg(unix)]
    {
        #[allow(unused_imports)]
        use std::os::unix::process::CommandExt;
        if let Some(ref arg0) = exec_request.arg0 {
            cmd.arg0(arg0);
        }
    }

    // Start with the sandbox-transformed env, then overlay proxy env vars
    // so the child process routes traffic through the network proxy.
    let mut child_env = exec_request.env;
    if let Some(ref proxy) = proxy {
        proxy.apply_to_env(&mut child_env);
    }
    cmd.envs(&child_env);

    match cmd.status().await {
        Ok(status) => exit_code_from_status(status),
        Err(e) => {
            eprintln!("error: failed to execute command: {e}");
            ExitCode::from(1)
        }
    }
}
