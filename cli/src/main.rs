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
/// When --allow-read is used, the sandboxed process can only read user data
/// from the listed paths. System libraries and binaries remain loadable
/// (the process can execute) but their contents are not exposed to user code.
///
/// Examples:
///   zerobox-exec -- node -e "console.log('hello')"
///   zerobox-exec --allow-write=. -- node -e "require('fs').writeFileSync('out.txt','hi')"
///   zerobox-exec --allow-net -- curl https://example.com
///   zerobox-exec --allow-read=/tmp --allow-write=/tmp -- node script.js
///   zerobox-exec --allow-all -- bash -c "echo anything goes"
#[derive(Parser, Debug)]
#[command(name = "zerobox-exec", version, about, long_about = None)]
struct Cli {
    /// Restrict readable user data to these paths only (comma-separated).
    /// System libraries and binaries remain accessible for execution.
    /// By default all reads are allowed.
    #[arg(long, value_delimiter = ',', num_args = 1..)]
    allow_read: Option<Vec<PathBuf>>,

    /// Allow writing to these paths (comma-separated).
    /// Without a value, allows writing everywhere.
    #[arg(long, value_delimiter = ',', num_args = 0..)]
    allow_write: Option<Vec<PathBuf>>,

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

/// Pre-resolved paths from CLI flags. Resolved once, used by both policy builders.
struct ResolvedPaths {
    readable: Option<Vec<AbsolutePathBuf>>,
    writable: Option<Vec<AbsolutePathBuf>>,
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

    let (writable, full_write) = match &cli.allow_write {
        Some(paths) if paths.is_empty() => (None, true),
        Some(paths) => (Some(resolve_all(cwd, paths)?), false),
        None => (None, false),
    };

    Ok(ResolvedPaths {
        readable,
        writable,
        full_write,
    })
}

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
        Some(paths) => entries.extend(make_path_entries(paths, FileSystemAccessMode::Read)),
        None => entries.push(make_root_entry(FileSystemAccessMode::Read)),
    }

    if resolved.full_write {
        entries.push(make_root_entry(FileSystemAccessMode::Write));
    } else if let Some(paths) = &resolved.writable {
        entries.extend(make_path_entries(paths, FileSystemAccessMode::Write));
    }

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

// ---------------------------------------------------------------------------
// macOS: custom seatbelt policy for --allow-read
//
// The Codex SandboxManager can produce either "full disk read" or "platform
// defaults + specific paths" policies. Neither gives us what --allow-read
// needs: system libraries loadable for execution, but user-visible file reads
// restricted to the listed paths only.
//
// When --allow-read is used on macOS, we build the seatbelt policy directly
// instead of going through SandboxManager.
// ---------------------------------------------------------------------------

/// Return the path plus its canonical form (if different) so seatbelt rules
/// cover both the symlink and the physical path (e.g. /tmp and /private/tmp).
#[cfg(target_os = "macos")]
fn seatbelt_path_variants(path: &Path) -> Vec<String> {
    let original = path.to_string_lossy().to_string();
    let mut variants = vec![original.clone()];
    if let Ok(canonical) = std::fs::canonicalize(path) {
        let canon_str = canonical.to_string_lossy().to_string();
        if canon_str != original {
            variants.push(canon_str);
        }
    }
    variants
}

#[cfg(target_os = "macos")]
fn build_restricted_read_seatbelt_policy(
    user_read_paths: &[AbsolutePathBuf],
    user_write_paths: &[AbsolutePathBuf],
    cwd: &Path,
    allow_net: bool,
) -> String {
    let mut policy = String::from(
        r#"(version 1)
(deny default)

; ── Process basics ──
(allow process*)
(allow sysctl-read)
(allow mach-lookup)
(allow ipc-posix-shm-read*)
(allow system-mac-syscall)
(allow system-fsctl)

; ── System libraries and frameworks (file-read* for dyld loading) ──
(allow file-read* file-map-executable file-test-existence
  (literal "/")
  (subpath "/System")
  (subpath "/usr/lib")
  (subpath "/usr/share")
  (subpath "/usr/libexec")
  (subpath "/Library/Apple")
  (subpath "/Library/Preferences")
  (subpath "/private/var/db")
  (subpath "/opt/homebrew/lib")
  (subpath "/usr/local/lib")
  (subpath "/Applications"))

; ── Binaries (readable + executable) ──
(allow file-read* file-test-existence
  (subpath "/usr/bin")
  (subpath "/usr/sbin")
  (subpath "/bin")
  (subpath "/sbin"))

; ── Config dirs: metadata only (stat works, open+read denied) ──
(allow file-read-metadata file-test-existence
  (subpath "/etc")
  (subpath "/private/etc")
  (subpath "/private"))

; ── Devices and terminal ──
(allow file-read* file-write* file-ioctl
  (regex #"^/dev/ttys[0-9]+$")
  (literal "/dev/tty")
  (literal "/dev/null")
  (literal "/dev/zero")
  (literal "/dev/ptmx"))
(allow file-read* file-write* (subpath "/dev/fd"))
(allow file-read-metadata (subpath "/dev"))
(allow file-read-data
  (literal "/dev/random")
  (literal "/dev/urandom"))

; ── Path traversal (firmlinks, symlinks) ──
(allow file-read-metadata file-test-existence
  (subpath "/System/Volumes")
  (literal "/tmp")
  (literal "/var"))

; ── Syslog ──
(allow network-outbound (literal "/private/var/run/syslog"))

"#,
    );

    // CWD: node/python/etc. need getcwd() which requires file-read-data on CWD.
    policy.push_str("; ── CWD (getcwd requires read access) ──\n");
    for p in seatbelt_path_variants(cwd) {
        policy.push_str(&format!(
            "(allow file-read* file-test-existence (subpath \"{p}\"))\n"
        ));
    }
    policy.push('\n');

    // User-specified read paths.
    // On macOS, symlinks like /tmp → /private/tmp must be resolved because
    // seatbelt checks the physical path after symlink resolution.
    if !user_read_paths.is_empty() {
        policy.push_str("; ── User data: allowed reads ──\n");
        for path in user_read_paths {
            for p in seatbelt_path_variants(path.as_path()) {
                policy.push_str(&format!("(allow file-read* (subpath \"{p}\"))\n"));
            }
        }
        policy.push('\n');
    }

    // User-specified write paths.
    if !user_write_paths.is_empty() {
        policy.push_str("; ── User data: allowed writes ──\n");
        for path in user_write_paths {
            for p in seatbelt_path_variants(path.as_path()) {
                policy.push_str(&format!(
                    "(allow file-read* file-write* (subpath \"{p}\"))\n"
                ));
            }
        }
        policy.push('\n');
    }

    // Network.
    if allow_net {
        policy.push_str(
            "; ── Network ──\n\
             (allow network*)\n\
             (allow system-socket)\n\
             ; TLS needs OpenSSL config and certificates\n\
             (allow file-read* (subpath \"/private/etc/ssl\"))\n\
             (allow file-read* (subpath \"/etc/ssl\"))\n\
             (allow file-read* (literal \"/private/etc/resolv.conf\"))\n\n",
        );
    }

    policy
}

/// Map process exit status to an exit code, preserving signal information.
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

async fn spawn_and_wait(argv: &[String], cwd: &Path, env: &HashMap<String, String>) -> ExitCode {
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.current_dir(cwd);
    cmd.env_clear();
    for (k, v) in env {
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

    let env: HashMap<String, String> = std::env::vars().collect();

    // ── Restricted-read path (macOS): build seatbelt policy directly ──
    //
    // When --allow-read is used, the Codex SandboxManager can't produce the
    // policy we need (system libs loadable but user reads restricted). We
    // build the sandbox-exec invocation ourselves.
    #[cfg(target_os = "macos")]
    if resolved.readable.is_some() && !cli.allow_all && !cli.no_sandbox {
        let user_reads = resolved.readable.as_deref().unwrap_or_default();
        let user_writes = resolved.writable.as_deref().unwrap_or_default();

        let policy =
            build_restricted_read_seatbelt_policy(user_reads, user_writes, &cwd, cli.allow_net);

        let mut argv = vec![
            "/usr/bin/sandbox-exec".to_string(),
            "-p".to_string(),
            policy,
            "--".to_string(),
        ];
        argv.extend(cli.command.iter().cloned());

        return spawn_and_wait(&argv, &cwd, &env).await;
    }

    // ── Default path: use Codex SandboxManager ──
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

    let manager = SandboxManager::new();
    let request = SandboxTransformRequest {
        command: SandboxCommand {
            program,
            args,
            cwd: cwd.clone(),
            env: env.clone(),
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

    spawn_and_wait(&exec_request.command, &cwd, &exec_request.env).await
}
