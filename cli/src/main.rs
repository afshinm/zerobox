mod env;
mod policy;
mod proxy;
mod secret;

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
/// Permissions are deny-by-default for writes, network, and environment
/// variables. Reads are allowed everywhere unless restricted with
/// --allow-read=<paths>.
///
/// Deny flags carve out exceptions within allowed paths and always take
/// precedence over allow flags.
///
/// Examples:
///   zerobox -- node -e "console.log('hello')"
///   zerobox --allow-write=. --deny-write=./.git -- node script.js
///   zerobox --allow-net=example.com -- node script.js
///   zerobox --env FOO=bar -- node script.js
///   zerobox --secret API_KEY=sk-123 --secret-host API_KEY=api.openai.com -- node agent.js
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

    /// Require full sandbox (bubblewrap on Linux). Fail instead of falling
    /// back to weaker isolation (Landlock) when namespaces are unavailable.
    #[arg(long)]
    pub strict_sandbox: bool,

    /// Set environment variables for the sandboxed command (KEY=VALUE).
    /// Can be specified multiple times. These always survive env filtering.
    #[arg(long = "env", value_name = "KEY=VALUE")]
    pub set_env: Vec<String>,

    /// Inherit parent environment variables (comma-separated).
    /// By default only PATH, HOME, USER, SHELL, TERM, LANG are inherited.
    /// Without a value, inherits all. With values, inherits only those.
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    pub allow_env: Option<Vec<String>>,

    /// Drop these parent environment variables (comma-separated).
    /// Takes precedence over --allow-env.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    pub deny_env: Option<Vec<String>>,

    /// Secret key-value pairs (KEY=VALUE). The real value is held by the proxy;
    /// the sandboxed process sees a random placeholder in the env var.
    /// Implicitly enables network for hosts specified with --secret-host
    /// (or all hosts if --secret-host is not set). Can be specified multiple times.
    #[arg(long = "secret", value_name = "KEY=VALUE")]
    pub secret: Vec<String>,

    /// Restrict a secret to specific hosts (KEY=host1,host2).
    /// Without this, the secret is substituted for all hosts.
    #[arg(long = "secret-host", value_name = "KEY=HOSTS")]
    pub secret_host: Vec<String>,

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

    let secret_store = match secret::parse_secret_flags(&cli.secret, &cli.secret_host) {
        Ok(store) => std::sync::Arc::new(store),
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

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

    let net_enabled = net_is_enabled(&cli) || !secret_store.is_empty();
    let fs_policy = build_fs_policy(&resolved, cli.allow_all, net_enabled);
    let net_policy = build_net_policy(&cli);
    let legacy_policy = build_legacy_sandbox_policy(&resolved, &cli);

    // Ensure Rustls crypto provider is initialized (required for MITM/TLS).
    codex_utils_rustls_provider::ensure_rustls_crypto_provider();

    let proxy = match build_network_proxy(&cli, &secret_store).await {
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

    let manager = SandboxManager::new();
    let request = SandboxTransformRequest {
        command: SandboxCommand {
            program: cli.command[0].clone(),
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
        #[cfg(target_os = "macos")]
        macos_seatbelt_profile_extensions: None,
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

    // Use output() instead of status() to capture and relay stdout/stderr.
    // On Linux bwrap, inherited stdio can lose buffered output from runtimes
    // like Node.js when the bwrap process exits before the pipe is drained.
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
}
