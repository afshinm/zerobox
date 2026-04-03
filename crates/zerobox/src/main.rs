mod debug;
mod env;
mod policy;
mod proxy;
mod secret;
mod snapshot;

use debug::debug_log;

#[cfg(target_os = "linux")]
use std::path::Path;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use codex_protocol::config_types::WindowsSandboxLevel;
use codex_sandboxing::{
    SandboxCommand, SandboxManager, SandboxTransformRequest, SandboxType, get_platform_sandbox,
};

use policy::{
    build_fs_policy, build_legacy_sandbox_policy, build_net_policy, net_is_enabled,
    resolve_cli_paths,
};
use proxy::build_network_proxy;

/// Sandbox any command with file, network, and credential controls.
#[derive(Parser, Debug)]
#[command(name = "zerobox", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub subcommand: Option<CliSubcommand>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub allow_read: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_read: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_write: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_write: Option<Vec<PathBuf>>,

    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_net: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_net: Option<Vec<String>>,

    /// Grant all permissions.
    #[arg(long, short = 'A')]
    pub allow_all: bool,

    /// Working directory for the sandboxed command.
    #[arg(long, short = 'C')]
    pub cwd: Option<PathBuf>,

    #[arg(long)]
    pub no_sandbox: bool,

    #[arg(long)]
    pub strict_sandbox: bool,

    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub set_env: Vec<String>,

    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_env: Option<Vec<String>>,

    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_env: Option<Vec<String>>,

    #[arg(long = "secret", value_name = "KEY=VALUE")]
    pub secret: Vec<String>,

    #[arg(long = "secret-host", value_name = "KEY=HOSTS")]
    pub secret_host: Vec<String>,

    #[arg(long)]
    pub debug: bool,

    /// Record filesystem changes during execution.
    #[arg(long)]
    pub snapshot: bool,

    /// Record and immediately undo all filesystem changes after exit.
    /// Implies --snapshot.
    #[arg(long)]
    pub restore: bool,

    #[arg(long = "snapshot-path", value_delimiter = ',', num_args = 1..)]
    pub snapshot_paths: Option<Vec<std::path::PathBuf>>,

    #[arg(long = "snapshot-exclude", value_delimiter = ',', num_args = 1..)]
    pub snapshot_exclude: Option<Vec<String>>,

    /// The command and arguments to run.
    #[arg(trailing_var_arg = true)]
    pub command: Vec<String>,
}

#[derive(Subcommand, Debug)]
pub enum CliSubcommand {
    /// Manage filesystem snapshots.
    Snapshot {
        #[command(subcommand)]
        action: SnapshotAction,
    },
}

#[derive(Subcommand, Debug)]
pub enum SnapshotAction {
    /// List snapshot sessions.
    List,
    /// Show changes in a session.
    Diff {
        /// Session ID.
        id: String,
    },
    /// Restore filesystem to a session's baseline.
    Restore {
        /// Session ID.
        id: String,
    },
    /// Remove old snapshot sessions.
    Clean {
        /// Remove sessions older than N days.
        #[arg(long, default_value = "30")]
        older_than: u64,
    },
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

/// Check if the current process can create user namespaces (required for bubblewrap).
/// Returns false inside Docker containers or on kernels with unprivileged_userns_clone=0.
#[cfg(target_os = "linux")]
fn can_create_user_namespace() -> bool {
    use std::process::Command;
    // Try unshare with user namespace — the lightest possible check.
    Command::new("unshare")
        .args(["--user", "--", "true"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "linux"))]
fn can_create_user_namespace() -> bool {
    true // Not applicable on macOS/Windows.
}

fn main() -> ExitCode {
    // Set CODEX_HOME before tokio spawns threads (set_var is unsafe with threads).
    if std::env::var("CODEX_HOME").is_err()
        && let Some(home) = dirs::home_dir()
    {
        let zerobox_home = home.join(".zerobox");
        let _ = std::fs::create_dir_all(&zerobox_home);
        // SAFETY: truly single-threaded here — tokio runtime not yet started.
        unsafe { std::env::set_var("CODEX_HOME", &zerobox_home) };
    }
    tokio_main()
}

#[tokio::main]
async fn tokio_main() -> ExitCode {
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

    if let Some(CliSubcommand::Snapshot { action }) = &cli.subcommand {
        return snapshot::handle_subcommand(action);
    }

    if cli.command.is_empty() {
        eprintln!("error: no command specified");
        return ExitCode::from(1);
    }

    let dbg = cli.debug;

    if dbg {
        debug::init_tracing();
    }

    let secret_store = match secret::parse_secret_flags(&cli.secret, &cli.secret_host) {
        Ok(store) => std::sync::Arc::new(store),
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    debug_log!(dbg, "secrets: {} configured", cli.secret.len());
    if secret_store.requires_mitm() {
        debug_log!(dbg, "secrets: MITM enabled for header substitution");
    }

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

    debug_log!(dbg, "cwd: {}", cwd.display());

    if cli.strict_sandbox && (cli.no_sandbox || cli.allow_all) {
        eprintln!("error: --strict-sandbox cannot be combined with --no-sandbox or --allow-all");
        return ExitCode::from(1);
    }

    let (sandbox_type, use_legacy_landlock) = if cli.no_sandbox || cli.allow_all {
        (SandboxType::None, false)
    } else {
        match get_platform_sandbox(false) {
            Some(SandboxType::LinuxSeccomp) => {
                if can_create_user_namespace() {
                    (SandboxType::LinuxSeccomp, false)
                } else if cli.strict_sandbox {
                    eprintln!(
                        "error: --strict-sandbox requires bubblewrap but user namespaces are unavailable.\n\
                         If running in Docker, start the container with:\n  \
                         docker run --cap-add SYS_ADMIN --security-opt seccomp=unconfined ..."
                    );
                    return ExitCode::from(1);
                } else {
                    eprintln!(
                        "warning: bubblewrap unavailable (no user namespaces), \
                         using landlock (reduced isolation — no PID/network namespace). \
                         Writes and network are still blocked by default."
                    );
                    (SandboxType::LinuxSeccomp, true)
                }
            }
            other => (other.unwrap_or(SandboxType::None), false),
        }
    };

    // Landlock fallback doesn't support custom file policies (split policies
    // require bwrap). Network filtering via the proxy still works.
    if use_legacy_landlock {
        let has_custom_policies = cli.allow_read.is_some()
            || cli.deny_read.is_some()
            || cli.allow_write.is_some()
            || cli.deny_write.is_some();
        if has_custom_policies {
            eprintln!(
                "error: custom file permissions (--allow-read, --allow-write, etc.) \
                 require bubblewrap, which is unavailable in this environment.\n\
                 The default sandbox (deny writes, deny network) still works.\n\
                 For full permissions control in Docker, start the container with:\n  \
                 docker run --cap-add SYS_ADMIN --security-opt seccomp=unconfined ..."
            );
            return ExitCode::from(1);
        }
    }

    debug_log!(
        dbg,
        "sandbox: {sandbox_type:?} (landlock_fallback={use_legacy_landlock})"
    );

    let net_enabled = net_is_enabled(&cli) || !secret_store.is_empty();
    let fs_policy = build_fs_policy(&resolved, cli.allow_all, net_enabled);
    let net_policy = build_net_policy(&cli);
    let legacy_policy = build_legacy_sandbox_policy(&resolved, &cli);

    debug_log!(
        dbg,
        "network: {}",
        if net_enabled { "enabled" } else { "blocked" }
    );
    if let Some(ref domains) = cli.allow_net {
        if domains.is_empty() {
            debug_log!(dbg, "network: all domains allowed");
        } else {
            debug_log!(dbg, "network: allowed domains: {}", domains.join(", "));
        }
    }
    if let Some(ref domains) = cli.deny_net {
        debug_log!(dbg, "network: denied domains: {}", domains.join(", "));
    }

    codex_utils_rustls_provider::ensure_rustls_crypto_provider();

    let proxy = match build_network_proxy(&cli, &secret_store, dbg).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: failed to build network proxy: {e:#}");
            return ExitCode::from(1);
        }
    };

    let _proxy_handle = if let Some(ref proxy) = proxy {
        match proxy.run().await {
            Ok(handle) => {
                debug_log!(dbg, "proxy: active");
                Some(handle)
            }
            Err(e) => {
                eprintln!("error: failed to start network proxy: {e:#}");
                return ExitCode::from(1);
            }
        }
    } else {
        debug_log!(dbg, "proxy: none");
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

    let mut child_env = match env::build_child_env(&cli) {
        Ok(e) => e,
        Err(msg) => {
            eprintln!("error: {msg}");
            return ExitCode::from(1);
        }
    };

    // Inject secret placeholders into child env (real values stay in the proxy).
    for (key, placeholder) in secret_store.get_env_overrides() {
        child_env.insert(key, placeholder);
    }

    debug_log!(dbg, "env: {} vars passed to child", child_env.len());

    let do_snapshot = cli.snapshot || cli.restore;
    let snapshot_state = if do_snapshot {
        match snapshot::build_snapshot_state(&cli, &cwd) {
            Ok(mut state) => match state.manager.create_baseline() {
                Ok(baseline) => {
                    debug_log!(
                        dbg,
                        "snapshot: baseline captured ({} files)",
                        baseline.files.len()
                    );
                    Some((state, baseline))
                }
                Err(e) => {
                    eprintln!("warning: snapshot baseline failed: {e:#}");
                    None
                }
            },
            Err(e) => {
                eprintln!("warning: snapshot setup failed: {e:#}");
                None
            }
        }
    } else {
        None
    };

    let manager = SandboxManager::new();
    let request = SandboxTransformRequest {
        command: SandboxCommand {
            program: cli.command[0].clone().into(),
            args: cli.command[1..].to_vec(),
            cwd: cwd.clone(),
            env: child_env,
            additional_permissions: None,
        },
        policy: &legacy_policy,
        file_system_policy: &fs_policy,
        network_policy: net_policy,
        sandbox: sandbox_type,
        enforce_managed_network: proxy.is_some(),
        network: proxy.as_ref(),
        sandbox_policy_cwd: &cwd,
        codex_linux_sandbox_exe: linux_sandbox_exe.as_ref(),
        use_legacy_landlock,
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
    if !net_enabled {
        child_env.insert(
            "CODEX_SANDBOX_NETWORK_DISABLED".to_string(),
            "1".to_string(),
        );
    }

    // When MITM is active (secrets configured), inject the proxy CA cert into
    // the child's trust store so HTTPS clients accept the intercepted certs.
    if secret_store.requires_mitm()
        && let Some(ca_path) = secret::mitm_ca_cert_path()
    {
        let ca = ca_path.to_string_lossy().to_string();
        child_env.insert("CURL_CA_BUNDLE".to_string(), ca.clone());
        child_env.insert("SSL_CERT_FILE".to_string(), ca.clone());
        child_env.insert("NODE_EXTRA_CA_CERTS".to_string(), ca.clone());
        child_env.insert("REQUESTS_CA_BUNDLE".to_string(), ca.clone());
        child_env.insert("CARGO_HTTP_CAINFO".to_string(), ca.clone());
        child_env.insert("GIT_SSL_CAINFO".to_string(), ca);
    }

    cmd.envs(&child_env);

    // Inherit stdio for TTY (interactive), capture for pipes (SDK/scripts).
    let exit = if std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        match cmd.status().await {
            Ok(status) => exit_code_from_status(status),
            Err(e) => {
                eprintln!("error: failed to execute command: {e}");
                ExitCode::from(1)
            }
        }
    } else {
        match cmd.output().await {
            Ok(output) => {
                use std::io::Write;
                let _ = std::io::stdout().write_all(&output.stdout);
                let _ = std::io::stderr().write_all(&output.stderr);
                exit_code_from_status(output.status)
            }
            Err(e) => {
                eprintln!("error: failed to execute command: {e}");
                ExitCode::from(1)
            }
        }
    };

    if let Some((mut state, baseline)) = snapshot_state {
        match state.manager.create_incremental(&baseline) {
            Ok((final_manifest, changes)) => {
                snapshot::print_summary(&changes);

                if cli.restore && !changes.is_empty() {
                    match state.manager.restore_to(&baseline) {
                        Ok(applied) => {
                            eprintln!("snapshot: restored {} files", applied.len());
                        }
                        Err(e) => {
                            eprintln!("snapshot: restore failed: {e:#}");
                        }
                    }
                }

                let mut meta = snapshot::build_session_metadata(&state, &cli, &baseline);
                meta.ended = Some(chrono::Utc::now().to_rfc3339());
                meta.snapshot_count = state.manager.snapshot_count();
                meta.merkle_roots.push(final_manifest.merkle_root);
                if let Err(e) = state.manager.save_session(&meta) {
                    eprintln!("snapshot: failed to save session: {e:#}");
                }
            }
            Err(e) => {
                eprintln!("snapshot: incremental failed: {e:#}");
            }
        }
    }

    exit
}
