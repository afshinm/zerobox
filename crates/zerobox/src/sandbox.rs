use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::process::Child;
use zerobox_protocol::config_types::WindowsSandboxLevel;
use zerobox_protocol::permissions::{
    FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry, FileSystemSandboxPolicy,
    FileSystemSpecialPath, NetworkSandboxPolicy,
};
use zerobox_protocol::protocol::SandboxPolicy;
use zerobox_sandboxing::{
    SandboxCommand, SandboxManager, SandboxTransformRequest, SandboxType, get_platform_sandbox,
};
use zerobox_utils_absolute_path::AbsolutePathBuf;

use crate::proxy;
use crate::secret;

pub(crate) const DEFAULT_ENV_KEYS: &[&str] = &["PATH", "HOME", "USER", "SHELL", "TERM", "LANG"];

pub struct SandboxOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

pub struct SandboxChild {
    inner: Child,
    _proxy_handle: Option<zerobox_network_proxy::NetworkProxyHandle>,
    _proxy: Option<zerobox_network_proxy::NetworkProxy>,
}

impl SandboxChild {
    pub fn stdout(&mut self) -> Option<tokio::process::ChildStdout> {
        self.inner.stdout.take()
    }

    pub fn stderr(&mut self) -> Option<tokio::process::ChildStderr> {
        self.inner.stderr.take()
    }

    pub async fn wait(mut self) -> Result<ExitStatus> {
        Ok(self.inner.wait().await?)
    }
}

pub struct Sandbox {
    program: String,
    args: Vec<String>,
    cwd: Option<PathBuf>,
    env: HashMap<String, String>,
    inherit_env: bool,
    allow_env: Option<Vec<String>>,
    deny_env: Vec<String>,
    allow_read: Vec<PathBuf>,
    deny_read: Vec<PathBuf>,
    allow_write: Vec<PathBuf>,
    deny_write: Vec<PathBuf>,
    full_write: bool,
    allow_net: Option<Vec<String>>,
    deny_net: Vec<String>,
    secrets: Vec<(String, String)>,
    secret_hosts: Vec<(String, String)>,
    disabled: bool,
    full_access: bool,
    strict: bool,
    profile_names: Vec<String>,
    use_profile: bool,
}

impl Sandbox {
    pub fn command(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: HashMap::new(),
            inherit_env: false,
            allow_env: None,
            deny_env: Vec::new(),
            allow_read: Vec::new(),
            deny_read: Vec::new(),
            allow_write: Vec::new(),
            deny_write: Vec::new(),
            full_write: false,
            allow_net: None,
            deny_net: Vec::new(),
            secrets: Vec::new(),
            secret_hosts: Vec::new(),
            disabled: false,
            full_access: false,
            strict: false,
            profile_names: Vec::new(),
            use_profile: true,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: &[impl AsRef<str>]) -> Self {
        self.args
            .extend(args.iter().map(|s| s.as_ref().to_string()));
        self
    }

    pub fn cwd(mut self, path: impl Into<PathBuf>) -> Self {
        self.cwd = Some(path.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    pub fn envs(
        mut self,
        vars: impl IntoIterator<Item = (impl Into<String>, impl Into<String>)>,
    ) -> Self {
        for (k, v) in vars {
            self.env.insert(k.into(), v.into());
        }
        self
    }

    pub fn inherit_env(mut self) -> Self {
        self.inherit_env = true;
        self
    }

    pub fn allow_env(mut self, keys: &[impl AsRef<str>]) -> Self {
        let list = self.allow_env.get_or_insert_with(Vec::new);
        list.extend(keys.iter().map(|s| s.as_ref().to_string()));
        self
    }

    pub fn deny_env(mut self, keys: &[impl AsRef<str>]) -> Self {
        self.deny_env
            .extend(keys.iter().map(|s| s.as_ref().to_string()));
        self
    }

    pub fn allow_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.allow_read.push(path.into());
        self
    }

    pub fn deny_read(mut self, path: impl Into<PathBuf>) -> Self {
        self.deny_read.push(path.into());
        self
    }

    pub fn allow_write(mut self, path: impl Into<PathBuf>) -> Self {
        self.allow_write.push(path.into());
        self
    }

    pub fn deny_write(mut self, path: impl Into<PathBuf>) -> Self {
        self.deny_write.push(path.into());
        self
    }

    pub fn allow_write_all(mut self) -> Self {
        self.full_write = true;
        self
    }

    pub fn allow_net(mut self, domains: &[impl AsRef<str>]) -> Self {
        let list = self.allow_net.get_or_insert_with(Vec::new);
        list.extend(domains.iter().map(|s| s.as_ref().to_string()));
        self
    }

    pub fn allow_net_all(mut self) -> Self {
        self.allow_net = Some(Vec::new());
        self
    }

    pub fn deny_net(mut self, domains: &[impl AsRef<str>]) -> Self {
        self.deny_net
            .extend(domains.iter().map(|s| s.as_ref().to_string()));
        self
    }

    pub fn secret(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.secrets.push((key.into(), value.into()));
        self
    }

    pub fn secret_host(mut self, key: impl Into<String>, hosts: impl Into<String>) -> Self {
        self.secret_hosts.push((key.into(), hosts.into()));
        self
    }

    pub fn no_sandbox(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn full_access(mut self) -> Self {
        self.full_access = true;
        self
    }

    pub fn strict(mut self) -> Self {
        self.strict = true;
        self
    }

    pub fn profile(mut self, name: impl Into<String>) -> Self {
        self.profile_names.push(name.into());
        self.use_profile = true;
        self
    }

    pub fn profiles(mut self, names: &[impl AsRef<str>]) -> Self {
        self.profile_names
            .extend(names.iter().map(|s| s.as_ref().to_string()));
        self.use_profile = true;
        self
    }

    pub fn no_profile(mut self) -> Self {
        self.use_profile = false;
        self
    }

    pub async fn run(self) -> Result<SandboxOutput> {
        let mut prepared = self.prepare().await?;
        let output = prepared
            .cmd
            .output()
            .await
            .context("failed to execute command")?;
        Ok(SandboxOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        })
    }

    pub async fn spawn(self) -> Result<SandboxChild> {
        let mut prepared = self.prepare().await?;
        prepared.cmd.stdout(std::process::Stdio::piped());
        prepared.cmd.stderr(std::process::Stdio::piped());
        prepared.cmd.stdin(std::process::Stdio::null());
        let child = prepared.cmd.spawn().context("failed to spawn command")?;
        Ok(SandboxChild {
            inner: child,
            _proxy_handle: prepared._proxy_handle,
            _proxy: prepared._proxy,
        })
    }

    pub async fn status(self) -> Result<ExitStatus> {
        let mut prepared = self.prepare().await?;
        let status = prepared
            .cmd
            .status()
            .await
            .context("failed to execute command")?;
        Ok(status)
    }

    /// Re-exec the current binary inside a sandbox.
    pub fn wrap_self() -> Result<Self> {
        let exe = std::env::current_exe().context("cannot determine current executable")?;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let mut sb = Self::command(exe.to_string_lossy());
        for arg in args {
            sb = sb.arg(arg);
        }
        Ok(sb)
    }

    pub fn is_sandboxed() -> bool {
        std::env::var("ZEROBOX_SANDBOXED").is_ok()
    }

    /// No-op if already sandboxed. Otherwise re-execs inside the sandbox and exits.
    pub async fn exec_or_continue(self) -> Result<()> {
        if Self::is_sandboxed() {
            return Ok(());
        }
        let sb = self.env("ZEROBOX_SANDBOXED", "1");
        let status = sb.status().await?;
        std::process::exit(status.code().unwrap_or(1));
    }

    pub async fn prepare(self) -> Result<PreparedCommand> {
        init_home();

        let Sandbox {
            program,
            args,
            cwd,
            mut env,
            inherit_env,
            mut allow_env,
            mut deny_env,
            mut allow_read,
            mut deny_read,
            mut allow_write,
            mut deny_write,
            mut full_write,
            mut allow_net,
            mut deny_net,
            mut secrets,
            mut secret_hosts,
            mut disabled,
            mut full_access,
            strict,
            profile_names,
            use_profile,
        } = self;

        let cwd = match cwd {
            Some(p) => p,
            None => std::env::current_dir().context("cannot determine working directory")?,
        };

        if use_profile && !disabled && !full_access {
            let profile = if profile_names.is_empty() {
                crate::profile_core::load_profile("default", &cwd)?
            } else {
                crate::profile_core::load_profiles(&profile_names, &cwd)?
            };
            apply_profile(
                &profile,
                &mut allow_read,
                &mut deny_read,
                &mut allow_write,
                &mut deny_write,
                &mut full_write,
                &mut allow_net,
                &mut deny_net,
                &mut env,
                &mut allow_env,
                &mut deny_env,
                &mut secrets,
                &mut secret_hosts,
                &mut disabled,
                &mut full_access,
            );
        }

        #[cfg(unix)]
        if use_profile
            && !disabled
            && !full_access
            && profile_names.iter().any(|n| is_claude_invocation(n))
            && let Some(home) = validated_home()
        {
            apply_claude_json_redirect(&home);
        }

        if !full_access {
            validate_paths(&allow_read, &deny_read, &allow_write, &deny_write, &cwd)?;
        }

        let secret_store = Arc::new(
            secret::build_secret_store(&secrets, &secret_hosts).map_err(|e| anyhow::anyhow!(e))?,
        );

        if secret_store.requires_mitm()
            && let Some(ca_path) = secret::mitm_ca_cert_path()
        {
            allow_read.push(ca_path);
        }

        let mut child_env = build_env(inherit_env, allow_env.as_deref(), &deny_env, &env);
        for (key, placeholder) in secret_store.get_env_overrides() {
            child_env.insert(key, placeholder);
        }

        let net_enabled = allow_net.is_some() || !secret_store.is_empty();
        let (sandbox_type, use_legacy_landlock) = if disabled || full_access {
            (SandboxType::None, false)
        } else {
            select_sandbox_type(strict)?
        };

        let fs_policy = build_fs_policy(
            &allow_read,
            &deny_read,
            &allow_write,
            &deny_write,
            full_write,
            full_access,
            net_enabled,
            &cwd,
        );
        let net_policy = if net_enabled {
            NetworkSandboxPolicy::Enabled
        } else {
            NetworkSandboxPolicy::Restricted
        };
        let legacy_policy =
            build_legacy_policy(&allow_write, full_access, full_write, net_enabled, &cwd);

        zerobox_utils_rustls_provider::ensure_rustls_crypto_provider();
        let deny_slice = if deny_net.is_empty() {
            None
        } else {
            Some(deny_net.as_slice())
        };
        let proxy = proxy::build_proxy(allow_net.as_deref(), deny_slice, &secret_store).await?;

        let _proxy_handle = match proxy {
            Some(ref p) => Some(p.run().await.context("failed to start network proxy")?),
            None => None,
        };

        let linux_sandbox_exe: Option<PathBuf> = if cfg!(target_os = "linux") {
            std::env::current_exe().ok()
        } else {
            None
        };

        let cwd_abs = AbsolutePathBuf::from_absolute_path(&cwd)
            .context("working directory must be absolute")?;

        let manager = SandboxManager::new();
        let exec_request = manager
            .transform(SandboxTransformRequest {
                command: SandboxCommand {
                    program: program.into(),
                    args,
                    cwd: cwd_abs,
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
                zerobox_linux_sandbox_exe: linux_sandbox_exe.as_deref(),
                use_legacy_landlock,
                windows_sandbox_level: WindowsSandboxLevel::default(),
                windows_sandbox_private_desktop: false,
            })
            .map_err(|e| anyhow::anyhow!("sandbox transform failed: {e}"))?;

        let mut cmd = tokio::process::Command::new(&exec_request.command[0]);
        cmd.args(&exec_request.command[1..]);
        cmd.current_dir(&cwd);
        cmd.env_clear();
        cmd.kill_on_drop(true);

        #[cfg(unix)]
        {
            #[allow(unused_imports)]
            use std::os::unix::process::CommandExt;
            if let Some(ref arg0) = exec_request.arg0 {
                cmd.arg0(arg0);
            }
        }

        let mut final_env = exec_request.env;
        if let Some(ref proxy) = proxy {
            proxy.apply_to_env(&mut final_env);
        }
        if !net_enabled {
            final_env.insert(
                "CODEX_SANDBOX_NETWORK_DISABLED".to_string(),
                "1".to_string(),
            );
        }
        if secret_store.requires_mitm()
            && let Some(ca_path) = secret::mitm_ca_cert_path()
        {
            let ca = ca_path.to_string_lossy().to_string();
            for var in &[
                "CURL_CA_BUNDLE",
                "SSL_CERT_FILE",
                "NODE_EXTRA_CA_CERTS",
                "REQUESTS_CA_BUNDLE",
                "CARGO_HTTP_CAINFO",
                "GIT_SSL_CAINFO",
                "GOOSE_CA_CERT_PATH",
            ] {
                final_env.insert(var.to_string(), ca.clone());
            }
        }
        cmd.envs(&final_env);

        Ok(PreparedCommand {
            cmd,
            _proxy_handle,
            _proxy: proxy,
        })
    }
}

pub struct PreparedCommand {
    cmd: tokio::process::Command,
    _proxy_handle: Option<zerobox_network_proxy::NetworkProxyHandle>,
    _proxy: Option<zerobox_network_proxy::NetworkProxy>,
}

impl PreparedCommand {
    pub fn into_command(self) -> tokio::process::Command {
        self.cmd
    }
}

#[allow(clippy::too_many_arguments)]
fn build_fs_policy(
    allow_read: &[PathBuf],
    deny_read: &[PathBuf],
    allow_write: &[PathBuf],
    deny_write: &[PathBuf],
    full_write: bool,
    full_access: bool,
    net_enabled: bool,
    cwd: &Path,
) -> FileSystemSandboxPolicy {
    if full_access {
        return FileSystemSandboxPolicy::unrestricted();
    }

    let mut entries: Vec<FileSystemSandboxEntry> = Vec::new();

    if allow_read.is_empty() {
        entries.push(FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Read,
        });
    } else {
        #[cfg(target_os = "linux")]
        entries.push(FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Minimal,
            },
            access: FileSystemAccessMode::Read,
        });
        for path in allow_read {
            if let Ok(abs) = resolve_path(cwd, path) {
                entries.push(FileSystemSandboxEntry {
                    path: FileSystemPath::Path { path: abs },
                    access: FileSystemAccessMode::Read,
                });
            }
        }
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
            && let Ok(abs) = AbsolutePathBuf::try_from(dir.to_path_buf())
        {
            entries.push(FileSystemSandboxEntry {
                path: FileSystemPath::Path { path: abs },
                access: FileSystemAccessMode::Read,
            });
        }
        if net_enabled {
            if let Ok(abs) = AbsolutePathBuf::try_from(crate::zerobox_home().join("tmp")) {
                entries.push(FileSystemSandboxEntry {
                    path: FileSystemPath::Path { path: abs },
                    access: FileSystemAccessMode::Read,
                });
            }
            if let Ok(abs) = AbsolutePathBuf::try_from(PathBuf::from("/run")) {
                entries.push(FileSystemSandboxEntry {
                    path: FileSystemPath::Path { path: abs },
                    access: FileSystemAccessMode::Read,
                });
            }
        }
    }

    for path in deny_read {
        if let Ok(abs) = resolve_path(cwd, path) {
            entries.push(FileSystemSandboxEntry {
                path: FileSystemPath::Path { path: abs },
                access: FileSystemAccessMode::None,
            });
        }
    }

    if full_write {
        entries.push(FileSystemSandboxEntry {
            path: FileSystemPath::Special {
                value: FileSystemSpecialPath::Root,
            },
            access: FileSystemAccessMode::Write,
        });
    } else {
        for path in allow_write {
            if let Ok(abs) = resolve_path(cwd, path) {
                entries.push(FileSystemSandboxEntry {
                    path: FileSystemPath::Path { path: abs },
                    access: FileSystemAccessMode::Write,
                });
            }
        }
    }

    for path in deny_write {
        if let Ok(abs) = resolve_path(cwd, path) {
            entries.push(FileSystemSandboxEntry {
                path: FileSystemPath::Path { path: abs },
                access: FileSystemAccessMode::Read,
            });
        }
    }

    FileSystemSandboxPolicy::restricted(entries)
}

fn build_legacy_policy(
    allow_write: &[PathBuf],
    full_access: bool,
    full_write: bool,
    net_enabled: bool,
    cwd: &Path,
) -> SandboxPolicy {
    if full_access || full_write {
        return SandboxPolicy::DangerFullAccess;
    }
    if !allow_write.is_empty() {
        let writable_roots: Vec<AbsolutePathBuf> = allow_write
            .iter()
            .filter_map(|p| resolve_path(cwd, p).ok())
            .collect();
        SandboxPolicy::WorkspaceWrite {
            writable_roots,
            read_only_access: Default::default(),
            network_access: net_enabled,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    } else {
        SandboxPolicy::ReadOnly {
            access: Default::default(),
            network_access: net_enabled,
        }
    }
}

fn build_env(
    inherit: bool,
    allow_env: Option<&[String]>,
    deny_env: &[String],
    overrides: &HashMap<String, String>,
) -> HashMap<String, String> {
    let parent: HashMap<String, String> = std::env::vars().collect();
    let mut env = if inherit {
        parent
    } else if let Some(keys) = allow_env {
        if keys.is_empty() {
            parent
        } else {
            let set: std::collections::HashSet<&str> = keys.iter().map(|s| s.as_str()).collect();
            parent
                .into_iter()
                .filter(|(k, _)| set.contains(k.as_str()))
                .collect()
        }
    } else {
        parent
            .into_iter()
            .filter(|(k, _)| DEFAULT_ENV_KEYS.contains(&k.as_str()))
            .collect()
    };
    for key in deny_env {
        env.remove(key);
    }
    env.extend(overrides.iter().map(|(k, v)| (k.clone(), v.clone())));
    env
}

#[allow(clippy::too_many_arguments)]
fn apply_profile(
    profile: &crate::profile_core::Profile,
    allow_read: &mut Vec<PathBuf>,
    deny_read: &mut Vec<PathBuf>,
    allow_write: &mut Vec<PathBuf>,
    deny_write: &mut Vec<PathBuf>,
    full_write: &mut bool,
    allow_net: &mut Option<Vec<String>>,
    deny_net: &mut Vec<String>,
    env: &mut HashMap<String, String>,
    allow_env: &mut Option<Vec<String>>,
    deny_env: &mut Vec<String>,
    secrets: &mut Vec<(String, String)>,
    secret_hosts: &mut Vec<(String, String)>,
    disabled: &mut bool,
    full_access: &mut bool,
) {
    fn merge_paths(target: &mut Vec<PathBuf>, source: &Option<Vec<String>>) {
        if let Some(paths) = source {
            for p in paths {
                let pb = PathBuf::from(p);
                if !target.contains(&pb) {
                    target.push(pb);
                }
            }
        }
    }

    fn merge_strings(target: &mut Vec<String>, source: &Option<Vec<String>>) {
        if let Some(items) = source {
            for s in items {
                if !target.contains(s) {
                    target.push(s.clone());
                }
            }
        }
    }

    fn merge_optional_strings(target: &mut Option<Vec<String>>, source: &Option<Vec<String>>) {
        if let Some(items) = source {
            let list = target.get_or_insert_with(Vec::new);
            for s in items {
                if !list.contains(s) {
                    list.push(s.clone());
                }
            }
        }
    }

    merge_paths(allow_read, &profile.allow_read);
    merge_paths(deny_read, &profile.deny_read);
    merge_paths(allow_write, &profile.allow_write);
    merge_paths(deny_write, &profile.deny_write);
    merge_strings(deny_net, &profile.deny_net);
    merge_strings(deny_env, &profile.deny_env);
    merge_optional_strings(allow_net, &profile.allow_net);
    merge_optional_strings(allow_env, &profile.allow_env);

    if let Some(ref profile_env) = profile.set_env {
        for (k, v) in profile_env {
            env.entry(k.clone()).or_insert_with(|| v.clone());
        }
    }

    if let Some(ref hosts_map) = profile.secret_hosts {
        for (key, hosts) in hosts_map {
            secret_hosts.push((key.clone(), hosts.join(",")));
        }
    }

    if profile.allow_all == Some(true) {
        *full_access = true;
    }
    if profile.no_sandbox == Some(true) {
        *disabled = true;
    }
    let _ = (full_write, secrets);
}

fn validate_paths(
    allow_read: &[PathBuf],
    deny_read: &[PathBuf],
    allow_write: &[PathBuf],
    deny_write: &[PathBuf],
    cwd: &Path,
) -> Result<()> {
    for p in allow_read {
        resolve_path(cwd, p)
            .with_context(|| format!("invalid allow_read path: {}", p.display()))?;
    }
    for p in deny_read {
        resolve_path(cwd, p).with_context(|| format!("invalid deny_read path: {}", p.display()))?;
    }
    for p in allow_write {
        resolve_path(cwd, p)
            .with_context(|| format!("invalid allow_write path: {}", p.display()))?;
    }
    for p in deny_write {
        resolve_path(cwd, p)
            .with_context(|| format!("invalid deny_write path: {}", p.display()))?;
    }
    Ok(())
}

fn init_home() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let home = crate::zerobox_home();
        let _ = std::fs::create_dir_all(&home);
    });
}

/// True if `profile_name` is `"claude"` or transitively composes it via `use:`.
fn is_claude_invocation(profile_name: &str) -> bool {
    is_claude_invocation_with(profile_name, crate::profile_core::load_profile_uses)
}

fn is_claude_invocation_with<F>(profile_name: &str, load_uses: F) -> bool
where
    F: Fn(&str) -> Option<Vec<String>>,
{
    let mut visited: Vec<String> = Vec::new();
    let mut stack: Vec<String> = vec![profile_name.to_string()];
    while let Some(name) = stack.pop() {
        if name == "claude" {
            return true;
        }
        if visited.iter().any(|v| v == &name) {
            continue;
        }
        visited.push(name.clone());
        if let Some(uses) = load_uses(&name) {
            stack.extend(uses);
        }
    }
    false
}

/// `HOME` must be an absolute path; otherwise a malicious parent could
/// redirect filesystem operations by setting `HOME=.` or similar.
#[cfg(unix)]
fn validated_home() -> Option<PathBuf> {
    validate_home_str(std::env::var("HOME").ok().as_deref())
}

#[cfg(unix)]
fn validate_home_str(home: Option<&str>) -> Option<PathBuf> {
    let home = home?;
    if home.is_empty() {
        return None;
    }
    let path = PathBuf::from(home);
    if !path.is_absolute() {
        return None;
    }
    Some(path)
}

/// Claude Code writes `~/.claude.json` atomically via temp files named
/// `~/.claude.json.tmp.<pid>.<timestamp>`. Sandboxes grant access to fixed
/// paths, not patterns, so those temp writes are denied and auth-token
/// refresh silently breaks. Redirecting `~/.claude.json` through a symlink
/// into `~/.claude/` makes the temp files land in a directory that's
/// granted as a whole.
#[cfg(unix)]
fn apply_claude_json_redirect(home: &Path) {
    use std::os::unix::fs::OpenOptionsExt;

    let precreate = |path: &Path, is_dir: bool| {
        let result = if is_dir {
            std::fs::create_dir_all(path)
        } else {
            std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(path)
                .map(|_| ())
        };
        if let Err(e) = result
            && e.kind() != std::io::ErrorKind::AlreadyExists
        {
            eprintln!("warning: failed to pre-create {}: {e}", path.display());
        }
    };

    precreate(&home.join(".claude.json.lock"), false);
    precreate(&home.join(".cache/claude-cli-nodejs"), true);

    let claude_json = home.join(".claude.json");
    let claude_dir = home.join(".claude");
    let redirect_target = claude_dir.join("claude.json");

    if let Err(e) = std::fs::create_dir_all(&claude_dir) {
        eprintln!("warning: failed to create {}: {e}", claude_dir.display());
        return;
    }

    if claude_json.is_symlink() {
        return;
    }

    if claude_json.exists() {
        if redirect_target.exists() {
            eprintln!(
                "warning: cannot redirect claude config — both {} and {} exist \
                 with independent content. Compare the two, keep the current one, \
                 delete the other, then re-run.",
                claude_json.display(),
                redirect_target.display()
            );
            return;
        }
        if let Err(e) = std::fs::rename(&claude_json, &redirect_target) {
            eprintln!(
                "warning: failed to move {} to {}: {e}",
                claude_json.display(),
                redirect_target.display()
            );
            return;
        }
    } else {
        precreate(&redirect_target, false);
    }

    if let Err(e) = std::os::unix::fs::symlink(".claude/claude.json", &claude_json)
        && e.kind() != std::io::ErrorKind::AlreadyExists
    {
        eprintln!(
            "warning: failed to create symlink {}: {e}",
            claude_json.display()
        );
    }
}

pub(crate) fn resolve_path(base: &Path, p: &Path) -> Result<AbsolutePathBuf> {
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    };
    AbsolutePathBuf::try_from(abs).context("failed to resolve path")
}

fn select_sandbox_type(strict: bool) -> Result<(SandboxType, bool)> {
    match get_platform_sandbox(false) {
        Some(SandboxType::LinuxSeccomp) => {
            if can_create_user_namespace() {
                Ok((SandboxType::LinuxSeccomp, false))
            } else if strict {
                anyhow::bail!(
                    "strict sandbox requires bubblewrap but user namespaces are unavailable"
                )
            } else {
                Ok((SandboxType::LinuxSeccomp, true))
            }
        }
        other => Ok((other.unwrap_or(SandboxType::None), false)),
    }
}

#[cfg(target_os = "linux")]
fn can_create_user_namespace() -> bool {
    use std::process::Command;
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
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn fs(
        ar: &[&str],
        dr: &[&str],
        aw: &[&str],
        dw: &[&str],
        fw: bool,
        fa: bool,
        net: bool,
    ) -> FileSystemSandboxPolicy {
        let ar: Vec<_> = ar.iter().map(|s| p(s)).collect();
        let dr: Vec<_> = dr.iter().map(|s| p(s)).collect();
        let aw: Vec<_> = aw.iter().map(|s| p(s)).collect();
        let dw: Vec<_> = dw.iter().map(|s| p(s)).collect();
        build_fs_policy(&ar, &dr, &aw, &dw, fw, fa, net, Path::new("/work"))
    }

    // filesystem policy

    #[test]
    fn fs_default_full_read_no_write() {
        let pol = fs(&[], &[], &[], &[], false, false, false);
        assert!(pol.has_full_disk_read_access());
        assert!(!pol.has_full_disk_write_access());
    }

    #[test]
    fn fs_full_access_unrestricted() {
        let pol = fs(&[], &[], &[], &[], false, true, false);
        assert!(pol.has_full_disk_read_access());
        assert!(pol.has_full_disk_write_access());
    }

    #[test]
    fn fs_full_access_ignores_denies() {
        let pol = fs(&[], &["/secret"], &[], &["/protected"], false, true, false);
        assert!(pol.has_full_disk_read_access());
        assert!(pol.has_full_disk_write_access());
    }

    #[test]
    fn fs_allow_read_restricts() {
        let pol = fs(&["/src"], &[], &[], &[], false, false, false);
        assert!(!pol.has_full_disk_read_access());
    }

    #[test]
    fn fs_deny_read_carves_from_default() {
        let pol = fs(&[], &["/secret"], &[], &[], false, false, false);
        let cwd = Path::new("/work");
        assert!(!pol.has_full_disk_read_access());
        assert!(pol.can_read_path_with_cwd(Path::new("/other"), cwd));
        assert!(!pol.can_read_path_with_cwd(Path::new("/secret/key"), cwd));
    }

    #[test]
    fn fs_deny_read_within_allow_read() {
        let pol = fs(&["/src"], &["/src/private"], &[], &[], false, false, false);
        let cwd = Path::new("/work");
        assert!(pol.can_read_path_with_cwd(Path::new("/src/lib.rs"), cwd));
        assert!(!pol.can_read_path_with_cwd(Path::new("/src/private/key"), cwd));
    }

    #[test]
    fn fs_write_specific_path() {
        let cwd = Path::new("/project");
        let ar: Vec<PathBuf> = vec![];
        let aw = vec![p("/project/dist")];
        let pol = build_fs_policy(&ar, &[], &aw, &[], false, false, false, cwd);
        assert!(pol.can_write_path_with_cwd(Path::new("/project/dist/out.js"), cwd));
        assert!(!pol.can_write_path_with_cwd(Path::new("/project/src/main.rs"), cwd));
    }

    #[test]
    fn fs_full_write() {
        let pol = fs(&[], &[], &[], &[], true, false, false);
        assert!(pol.has_full_disk_write_access());
    }

    #[test]
    fn fs_deny_write_carves_from_allow() {
        let cwd = Path::new("/project");
        let aw = vec![p("/project")];
        let dw = vec![p("/project/.git")];
        let pol = build_fs_policy(&[], &[], &aw, &dw, false, false, false, cwd);
        assert!(pol.can_write_path_with_cwd(Path::new("/project/src/x"), cwd));
        assert!(!pol.can_write_path_with_cwd(Path::new("/project/.git/config"), cwd));
    }

    #[test]
    fn fs_deny_write_without_allow_is_noop() {
        let cwd = Path::new("/work");
        let pol = build_fs_policy(&[], &[], &[], &[p("/x")], false, false, false, cwd);
        assert!(!pol.can_write_path_with_cwd(Path::new("/x"), cwd));
        assert!(!pol.can_write_path_with_cwd(Path::new("/anywhere"), cwd));
    }

    #[test]
    fn fs_deny_read_and_deny_write_combined() {
        let cwd = Path::new("/work");
        let pol = build_fs_policy(
            &[],
            &[p("/secret")],
            &[p("/out")],
            &[p("/out/.git")],
            false,
            false,
            false,
            cwd,
        );
        assert!(!pol.can_read_path_with_cwd(Path::new("/secret/x"), cwd));
        assert!(pol.can_read_path_with_cwd(Path::new("/other"), cwd));
        assert!(pol.can_write_path_with_cwd(Path::new("/out/file"), cwd));
        assert!(!pol.can_write_path_with_cwd(Path::new("/out/.git/hooks"), cwd));
    }

    // legacy policy

    #[test]
    fn legacy_default_read_only_no_net() {
        let pol = build_legacy_policy(&[], false, false, false, Path::new("/tmp"));
        assert!(matches!(
            pol,
            SandboxPolicy::ReadOnly {
                network_access: false,
                ..
            }
        ));
    }

    #[test]
    fn legacy_net_flag_propagates() {
        let pol = build_legacy_policy(&[], false, false, true, Path::new("/tmp"));
        assert!(matches!(
            pol,
            SandboxPolicy::ReadOnly {
                network_access: true,
                ..
            }
        ));

        let pol = build_legacy_policy(&[p("/out")], false, false, true, Path::new("/tmp"));
        match pol {
            SandboxPolicy::WorkspaceWrite { network_access, .. } => assert!(network_access),
            other => panic!("expected WorkspaceWrite, got {other:?}"),
        }
    }

    #[test]
    fn legacy_write_paths_become_workspace_write() {
        let pol = build_legacy_policy(&[p("/tmp")], false, false, false, Path::new("/tmp"));
        assert!(matches!(pol, SandboxPolicy::WorkspaceWrite { .. }));
    }

    #[test]
    fn legacy_full_access_or_full_write_is_danger() {
        assert!(matches!(
            build_legacy_policy(&[], true, false, false, Path::new("/tmp")),
            SandboxPolicy::DangerFullAccess
        ));
        assert!(matches!(
            build_legacy_policy(&[], false, true, false, Path::new("/tmp")),
            SandboxPolicy::DangerFullAccess
        ));
        assert!(matches!(
            build_legacy_policy(&[], true, true, true, Path::new("/tmp")),
            SandboxPolicy::DangerFullAccess
        ));
    }

    // env

    #[test]
    fn env_default_filters_to_essentials() {
        let env = build_env(false, None, &[], &HashMap::new());
        for key in DEFAULT_ENV_KEYS {
            if std::env::var(key).is_ok() {
                assert!(env.contains_key(*key), "missing {key}");
            }
        }
        assert!(!env.contains_key("CARGO_MANIFEST_DIR"));
    }

    #[test]
    fn env_inherit_includes_all() {
        let env = build_env(true, None, &[], &HashMap::new());
        assert!(env.len() > DEFAULT_ENV_KEYS.len());
    }

    #[test]
    fn env_allow_env_specific_keys() {
        let keys = vec!["PATH".to_string()];
        let env = build_env(false, Some(&keys), &[], &HashMap::new());
        assert!(env.contains_key("PATH"));
        assert!(!env.contains_key("HOME"));
    }

    #[test]
    fn env_deny_removes_keys() {
        let deny = vec!["PATH".to_string()];
        let env = build_env(false, None, &deny, &HashMap::new());
        assert!(!env.contains_key("PATH"));
    }

    #[test]
    fn env_overrides_win() {
        let mut o = HashMap::new();
        o.insert("PATH".to_string(), "/custom".to_string());
        o.insert("NEW_VAR".to_string(), "val".to_string());
        let env = build_env(false, None, &[], &o);
        assert_eq!(env["PATH"], "/custom");
        assert_eq!(env["NEW_VAR"], "val");
    }

    // resolve_path

    #[test]
    fn resolve_absolute_unchanged() {
        let r = resolve_path(Path::new("/base"), Path::new("/abs/path")).unwrap();
        assert_eq!(r.as_path(), Path::new("/abs/path"));
    }

    #[test]
    fn resolve_relative_joined() {
        let r = resolve_path(Path::new("/base"), Path::new("child")).unwrap();
        assert_eq!(r.as_path(), Path::new("/base/child"));
    }

    // build_secret_store (separate code path from parse_secret_flags)

    #[test]
    fn secret_store_generates_placeholders() {
        let store = secret::build_secret_store(&[("K".into(), "v".into())], &[]).unwrap();
        let o = store.get_env_overrides();
        assert!(o["K"].starts_with("ZEROBOX_SECRET_"));
        assert_eq!(o["K"].len(), "ZEROBOX_SECRET_".len() + 64);
    }

    #[test]
    fn secret_store_host_binding() {
        let store = secret::build_secret_store(
            &[("K".into(), "v".into())],
            &[("K".into(), "a.com,b.com".into())],
        )
        .unwrap();
        let hosts = store.get_allowed_hosts();
        assert!(hosts.contains(&"a.com".to_string()));
        assert!(hosts.contains(&"b.com".to_string()));
    }

    #[test]
    fn secret_store_rejects_duplicate_keys() {
        assert!(
            secret::build_secret_store(&[("K".into(), "a".into()), ("K".into(), "b".into())], &[],)
                .is_err()
        );
    }

    #[test]
    fn secret_store_rejects_empty_key() {
        assert!(secret::build_secret_store(&[("".into(), "v".into())], &[],).is_err());
    }

    #[test]
    fn secret_store_rejects_unknown_host_key() {
        assert!(
            secret::build_secret_store(
                &[("A".into(), "v".into())],
                &[("B".into(), "x.com".into())],
            )
            .is_err()
        );
    }

    // builder

    #[test]
    fn builder_sets_all_fields() {
        let s = Sandbox::command("echo")
            .arg("hello")
            .args(&["world"])
            .cwd("/tmp")
            .env("K", "V")
            .envs([("A", "1")])
            .inherit_env()
            .allow_read("/src")
            .deny_read("/secret")
            .allow_write("/out")
            .deny_write("/out/.git")
            .allow_write_all()
            .allow_net(&["a.com"])
            .deny_net(&["evil.com"])
            .secret("KEY", "val")
            .secret_host("KEY", "api.com")
            .no_sandbox()
            .full_access()
            .profile("workspace")
            .no_profile();

        assert_eq!(s.program, "echo");
        assert_eq!(s.args, vec!["hello", "world"]);
        assert_eq!(s.cwd, Some(PathBuf::from("/tmp")));
        assert_eq!(s.env["K"], "V");
        assert_eq!(s.env["A"], "1");
        assert!(s.inherit_env);
        assert_eq!(s.allow_read, vec![p("/src")]);
        assert_eq!(s.deny_read, vec![p("/secret")]);
        assert_eq!(s.allow_write, vec![p("/out")]);
        assert_eq!(s.deny_write, vec![p("/out/.git")]);
        assert!(s.full_write);
        assert_eq!(s.allow_net, Some(vec!["a.com".to_string()]));
        assert_eq!(s.deny_net, vec!["evil.com".to_string()]);
        assert_eq!(s.secrets, vec![("KEY".into(), "val".into())]);
        assert_eq!(s.secret_hosts, vec![("KEY".into(), "api.com".into())]);
        assert!(s.disabled);
        assert!(s.full_access);
        assert_eq!(s.profile_names, vec!["workspace".to_string()]);
        assert!(!s.use_profile);
    }

    #[test]
    fn builder_allow_net_accumulates_across_calls() {
        let s = Sandbox::command("x")
            .allow_net(&["a.com"])
            .allow_net(&["b.com", "c.com"]);
        assert_eq!(
            s.allow_net,
            Some(vec!["a.com".into(), "b.com".into(), "c.com".into()])
        );
    }

    #[test]
    fn builder_allow_net_all_is_empty_some() {
        let s = Sandbox::command("x").allow_net_all();
        assert_eq!(s.allow_net, Some(vec![]));
    }

    #[test]
    fn builder_profile_accumulates_across_calls() {
        let s = Sandbox::command("x")
            .profile("workspace")
            .profile("git-config");
        assert_eq!(
            s.profile_names,
            vec!["workspace".to_string(), "git-config".to_string()]
        );
        assert!(s.use_profile);
    }

    #[test]
    fn builder_profiles_slice_extends() {
        let s = Sandbox::command("x").profiles(&["workspace", "git-config"]);
        assert_eq!(
            s.profile_names,
            vec!["workspace".to_string(), "git-config".to_string()]
        );
        assert!(s.use_profile);
    }

    #[test]
    fn builder_profile_and_profiles_compose() {
        let s = Sandbox::command("x")
            .profile("a")
            .profiles(&["b", "c"])
            .profile("d");
        assert_eq!(
            s.profile_names,
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string()
            ]
        );
    }

    // apply_profile

    fn apply_default_profile() -> (
        Vec<PathBuf>,
        Vec<PathBuf>,
        Vec<PathBuf>,
        Vec<PathBuf>,
        bool,
        Option<Vec<String>>,
        Vec<String>,
        HashMap<String, String>,
        Option<Vec<String>>,
        Vec<String>,
        Vec<(String, String)>,
        Vec<(String, String)>,
        bool,
        bool,
    ) {
        let cwd = std::env::current_dir().unwrap();
        let profile = crate::profile_core::load_profile("default", &cwd).unwrap();
        let mut ar = Vec::new();
        let mut dr = Vec::new();
        let mut aw = Vec::new();
        let mut dw = Vec::new();
        let mut fw = false;
        let mut an = None;
        let mut dn = Vec::new();
        let mut env = HashMap::new();
        let mut ae = None;
        let mut de = Vec::new();
        let mut sec = Vec::new();
        let mut sh = Vec::new();
        let mut dis = false;
        let mut fa = false;
        apply_profile(
            &profile, &mut ar, &mut dr, &mut aw, &mut dw, &mut fw, &mut an, &mut dn, &mut env,
            &mut ae, &mut de, &mut sec, &mut sh, &mut dis, &mut fa,
        );
        (ar, dr, aw, dw, fw, an, dn, env, ae, de, sec, sh, dis, fa)
    }

    #[test]
    fn profile_default_adds_deny_rules() {
        let (allow_read, deny_read, ..) = apply_default_profile();
        assert!(!deny_read.is_empty());
        assert!(!allow_read.is_empty());
        let home = dirs::home_dir().unwrap();
        assert!(deny_read.contains(&home.join(".ssh")));
    }

    #[test]
    fn profile_merges_with_existing_rules() {
        let cwd = std::env::current_dir().unwrap();
        let profile = crate::profile_core::load_profile("default", &cwd).unwrap();
        let mut allow_read = vec![p("/my/custom/path")];
        let mut deny_read = vec![p("/my/custom/deny")];
        let mut aw = Vec::new();
        let mut dw = Vec::new();
        let mut fw = false;
        let mut an = None;
        let mut dn = Vec::new();
        let mut env = HashMap::new();
        let mut ae = None;
        let mut de = Vec::new();
        let mut sec = Vec::new();
        let mut sh = Vec::new();
        let mut dis = false;
        let mut fa = false;
        apply_profile(
            &profile,
            &mut allow_read,
            &mut deny_read,
            &mut aw,
            &mut dw,
            &mut fw,
            &mut an,
            &mut dn,
            &mut env,
            &mut ae,
            &mut de,
            &mut sec,
            &mut sh,
            &mut dis,
            &mut fa,
        );
        assert!(allow_read.contains(&p("/my/custom/path")));
        assert!(deny_read.contains(&p("/my/custom/deny")));
        assert!(allow_read.len() > 1);
        assert!(deny_read.len() > 1);
    }

    // wrap_self / is_sandboxed

    #[test]
    fn wrap_self_captures_current_exe() {
        let sb = Sandbox::wrap_self().unwrap();
        let exe = std::env::current_exe().unwrap();
        assert_eq!(sb.program, exe.to_string_lossy().as_ref());
    }

    #[test]
    fn is_sandboxed_false_by_default() {
        assert!(!Sandbox::is_sandboxed());
    }

    // PreparedCommand

    #[test]
    fn prepared_command_into_command() {
        let cmd = tokio::process::Command::new("echo");
        let prepared = PreparedCommand {
            cmd,
            _proxy_handle: None,
            _proxy: None,
        };
        let cmd = prepared.into_command();
        assert!(format!("{cmd:?}").contains("echo"));
    }

    // is_claude_invocation

    #[test]
    fn is_claude_invocation_matches_profile_name() {
        assert!(is_claude_invocation("claude"));
    }

    #[test]
    fn is_claude_invocation_rejects_other_profiles() {
        // Mock loader so the test doesn't depend on real profile content or
        // user-dir overrides.
        let loader = |name: &str| -> Option<Vec<String>> {
            match name {
                "codex" => Some(vec!["workspace".to_string(), "codex-macos".to_string()]),
                "claude-macos" => Some(vec![]),
                "default" => Some(vec!["system-read-linux".to_string()]),
                "workspace" | "codex-macos" | "system-read-linux" => Some(vec![]),
                _ => None,
            }
        };
        assert!(!is_claude_invocation_with("codex", loader));
        assert!(!is_claude_invocation_with("claude-macos", loader));
        assert!(!is_claude_invocation_with("default", loader));
    }

    #[test]
    fn is_claude_invocation_follows_transitive_use() {
        let loader = |name: &str| -> Option<Vec<String>> {
            match name {
                "my-claude" => Some(vec!["claude".to_string()]),
                "nested" => Some(vec!["my-claude".to_string()]),
                "cycle-a" => Some(vec!["cycle-b".to_string()]),
                "cycle-b" => Some(vec!["cycle-a".to_string()]),
                _ => None,
            }
        };
        assert!(is_claude_invocation_with("my-claude", loader));
        assert!(is_claude_invocation_with("nested", loader));
        assert!(!is_claude_invocation_with("cycle-a", loader));
        assert!(!is_claude_invocation_with("unknown", loader));
    }

    #[test]
    fn is_claude_invocation_detected_in_multi_profile_list() {
        // Mirrors the expression used in `prepare()` for the claude redirect
        // gate: the redirect fires when any profile in the list resolves to
        // claude, directly or transitively.
        let loader = |name: &str| -> Option<Vec<String>> {
            match name {
                "custom-wrapper" => Some(vec!["claude".to_string()]),
                _ => None,
            }
        };
        let with_claude = ["workspace", "custom-wrapper", "git-config"];
        let without_claude = ["workspace", "git-config"];
        assert!(
            with_claude
                .iter()
                .any(|n| is_claude_invocation_with(n, loader))
        );
        assert!(
            !without_claude
                .iter()
                .any(|n| is_claude_invocation_with(n, loader))
        );
    }

    // validate_home_str

    #[cfg(unix)]
    #[test]
    fn validate_home_str_requires_absolute_path() {
        assert!(validate_home_str(None).is_none());
        assert!(validate_home_str(Some("")).is_none());
        assert!(validate_home_str(Some("relative/path")).is_none());
        assert!(validate_home_str(Some(".")).is_none());
        assert_eq!(validate_home_str(Some("/tmp")), Some(PathBuf::from("/tmp")));
        assert_eq!(
            validate_home_str(Some("/home/user")),
            Some(PathBuf::from("/home/user"))
        );
    }

    // apply_claude_json_redirect

    #[cfg(unix)]
    #[test]
    fn claude_json_redirect_creates_symlink_when_file_absent() {
        let tmp = tempfile::tempdir().unwrap();
        apply_claude_json_redirect(tmp.path());

        let link = tmp.path().join(".claude.json");
        let target = tmp.path().join(".claude/claude.json");
        assert!(link.is_symlink(), "~/.claude.json should be a symlink");
        assert!(target.exists(), "redirect target should be pre-created");
        assert!(tmp.path().join(".claude.json.lock").exists());
        assert!(tmp.path().join(".cache/claude-cli-nodejs").is_dir());
    }

    #[cfg(unix)]
    #[test]
    fn claude_json_redirect_moves_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let original = tmp.path().join(".claude.json");
        std::fs::write(&original, b"real-contents").unwrap();

        apply_claude_json_redirect(tmp.path());

        let target = tmp.path().join(".claude/claude.json");
        assert!(original.is_symlink());
        assert_eq!(std::fs::read(&target).unwrap(), b"real-contents");
    }

    #[cfg(unix)]
    #[test]
    fn claude_json_redirect_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        apply_claude_json_redirect(tmp.path());
        let target = tmp.path().join(".claude/claude.json");
        std::fs::write(&target, b"after-first-run").unwrap();

        apply_claude_json_redirect(tmp.path());

        assert!(tmp.path().join(".claude.json").is_symlink());
        assert_eq!(std::fs::read(&target).unwrap(), b"after-first-run");
    }

    #[cfg(unix)]
    #[test]
    fn claude_json_redirect_bails_when_both_exist() {
        let tmp = tempfile::tempdir().unwrap();
        let claude_json = tmp.path().join(".claude.json");
        let target_dir = tmp.path().join(".claude");
        let target = target_dir.join("claude.json");

        std::fs::create_dir_all(&target_dir).unwrap();
        std::fs::write(&claude_json, b"new-contents").unwrap();
        std::fs::write(&target, b"existing-contents").unwrap();

        apply_claude_json_redirect(tmp.path());

        assert!(!claude_json.is_symlink());
        assert!(claude_json.is_file());
        assert_eq!(std::fs::read(&claude_json).unwrap(), b"new-contents");
        assert_eq!(std::fs::read(&target).unwrap(), b"existing-contents");
    }
}
