mod policy;
mod proxy;

#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_sandboxing::{
    SandboxCommand, SandboxManager, SandboxTransformRequest, SandboxType, get_platform_sandbox,
};

use policy::{
    build_fs_policy, build_legacy_sandbox_policy, build_net_policy, net_is_enabled,
    resolve_cli_paths,
};
use proxy::build_network_proxy;

/// Run a command inside a cross-platform sandbox.
///
/// Permissions are deny-by-default for writes and network. Reads are allowed
/// everywhere unless restricted with --allow-read=<paths>.
///
/// Deny flags carve out exceptions within allowed paths and always take
/// precedence over allow flags.
///
/// Examples:
///   zerobox -- node -e "console.log('hello')"
///   zerobox --allow-write=. --deny-write=./.git -- node script.js
///   zerobox --allow-read=/tmp --allow-write=/tmp -- node script.js
///   zerobox --allow-net -- curl https://example.com
///   zerobox --allow-net=example.com,api.example.com -- node script.js
///   zerobox --allow-net --deny-net=evil.com -- node script.js
///   zerobox --allow-all -- bash -c "echo anything goes"
#[derive(Parser, Debug)]
#[command(name = "zerobox", version, about, long_about = None)]
pub struct Cli {
    /// Restrict readable user data to these paths only (comma-separated).
    /// System libraries and binaries remain accessible for execution.
    /// By default all reads are allowed.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub allow_read: Option<Vec<PathBuf>>,

    /// Block reading from these paths (comma-separated). Takes precedence
    /// over --allow-read.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_read: Option<Vec<PathBuf>>,

    /// Allow writing to these paths (comma-separated).
    /// Without a value, allows writing everywhere.
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_write: Option<Vec<PathBuf>>,

    /// Block writing to these paths (comma-separated). Takes precedence
    /// over --allow-write.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_write: Option<Vec<PathBuf>>,

    /// Allow outbound network access. Without a value, allows all domains.
    /// With values, restricts to specific domains (comma-separated).
    /// Examples: --allow-net, --allow-net=example.com,api.example.com
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_net: Option<Vec<String>>,

    /// Block network access to these domains (comma-separated).
    /// Takes precedence over --allow-net.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_net: Option<Vec<String>>,

    /// Grant all permissions (no sandbox). Use with caution.
    #[arg(long, short = 'A')]
    pub allow_all: bool,

    /// Working directory for the sandboxed command.
    #[arg(long, short = 'C')]
    pub cwd: Option<PathBuf>,

    /// Disable the sandbox entirely (just run the command).
    #[arg(long)]
    pub no_sandbox: bool,

    /// The command and arguments to run.
    #[arg(trailing_var_arg = true, required = true)]
    pub command: Vec<String>,
}

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

    let fs_policy = build_fs_policy(&resolved, cli.allow_all, net_is_enabled(&cli));
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
    cmd.kill_on_drop(true);

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

    // Build the child environment: sandbox-transformed env, proxy overlay,
    // and network-disabled signal for sandboxed processes.
    let mut child_env = exec_request.env;
    if let Some(ref proxy) = proxy {
        proxy.apply_to_env(&mut child_env);
    }
    if !net_is_enabled(&cli) {
        child_env.insert(
            "CODEX_SANDBOX_NETWORK_DISABLED".to_string(),
            "1".to_string(),
        );
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
