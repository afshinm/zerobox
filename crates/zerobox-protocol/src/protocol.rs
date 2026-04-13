use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use strum_macros::Display;
use tracing::error;
use ts_rs::TS;
use zerobox_utils_absolute_path::AbsolutePathBuf;

pub use crate::permissions::FileSystemAccessMode;
pub use crate::permissions::FileSystemPath;
pub use crate::permissions::FileSystemSandboxEntry;
pub use crate::permissions::FileSystemSandboxKind;
pub use crate::permissions::FileSystemSandboxPolicy;
pub use crate::permissions::FileSystemSpecialPath;
pub use crate::permissions::NetworkSandboxPolicy;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Display, Default, JsonSchema, TS,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
pub enum NetworkAccess {
    #[default]
    Restricted,
    Enabled,
}

impl NetworkAccess {
    pub fn is_enabled(self) -> bool {
        matches!(self, NetworkAccess::Enabled)
    }
}
fn default_include_platform_defaults() -> bool {
    true
}

/// Determines how read-only file access is granted inside a restricted
/// sandbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display, Default, JsonSchema, TS)]
#[strum(serialize_all = "kebab-case")]
#[serde(tag = "type", rename_all = "kebab-case")]
#[ts(tag = "type")]
pub enum ReadOnlyAccess {
    /// Restrict reads to an explicit set of roots.
    ///
    /// When `include_platform_defaults` is `true`, platform defaults required
    /// for basic execution are included in addition to `readable_roots`.
    Restricted {
        /// Include built-in platform read roots required for basic process
        /// execution.
        #[serde(default = "default_include_platform_defaults")]
        include_platform_defaults: bool,
        /// Additional absolute roots that should be readable.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        readable_roots: Vec<AbsolutePathBuf>,
    },

    /// Allow unrestricted file reads.
    #[default]
    FullAccess,
}

impl ReadOnlyAccess {
    pub fn has_full_disk_read_access(&self) -> bool {
        matches!(self, ReadOnlyAccess::FullAccess)
    }

    /// Returns true if platform defaults should be included for restricted read access.
    pub fn include_platform_defaults(&self) -> bool {
        matches!(
            self,
            ReadOnlyAccess::Restricted {
                include_platform_defaults: true,
                ..
            }
        )
    }

    /// Returns the readable roots for restricted read access.
    ///
    /// For [`ReadOnlyAccess::FullAccess`], returns an empty list because
    /// callers should grant blanket read access instead.
    pub fn get_readable_roots_with_cwd(&self, cwd: &Path) -> Vec<AbsolutePathBuf> {
        let mut roots: Vec<AbsolutePathBuf> = match self {
            ReadOnlyAccess::FullAccess => return Vec::new(),
            ReadOnlyAccess::Restricted { readable_roots, .. } => {
                let mut roots = readable_roots.clone();
                match AbsolutePathBuf::from_absolute_path(cwd) {
                    Ok(cwd_root) => roots.push(cwd_root),
                    Err(err) => {
                        error!("Ignoring invalid cwd {cwd:?} for sandbox readable root: {err}");
                    }
                }
                roots
            }
        };

        let mut seen = HashSet::new();
        roots.retain(|root| seen.insert(root.to_path_buf()));
        roots
    }
}

/// Execution restrictions for sandboxed commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Display, JsonSchema, TS)]
#[strum(serialize_all = "kebab-case")]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum SandboxPolicy {
    /// No restrictions whatsoever. Use with caution.
    #[serde(rename = "danger-full-access")]
    DangerFullAccess,

    /// Read-only access configuration.
    #[serde(rename = "read-only")]
    ReadOnly {
        /// Read access granted while running under this policy.
        #[serde(
            default,
            skip_serializing_if = "ReadOnlyAccess::has_full_disk_read_access"
        )]
        access: ReadOnlyAccess,

        /// When set to `true`, outbound network access is allowed. `false` by
        /// default.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        network_access: bool,
    },

    /// Indicates the process is already in an external sandbox. Allows full
    /// disk access while honoring the provided network setting.
    #[serde(rename = "external-sandbox")]
    ExternalSandbox {
        /// Whether the external sandbox permits outbound network traffic.
        #[serde(default)]
        network_access: NetworkAccess,
    },

    /// Same as `ReadOnly` but additionally grants write access to the current
    /// working directory ("workspace").
    #[serde(rename = "workspace-write")]
    WorkspaceWrite {
        /// Additional folders (beyond cwd and possibly TMPDIR) that should be
        /// writable from within the sandbox.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        writable_roots: Vec<AbsolutePathBuf>,

        /// Read access granted while running under this policy.
        #[serde(
            default,
            skip_serializing_if = "ReadOnlyAccess::has_full_disk_read_access"
        )]
        read_only_access: ReadOnlyAccess,

        /// When set to `true`, outbound network access is allowed. `false` by
        /// default.
        #[serde(default)]
        network_access: bool,

        /// When set to `true`, will NOT include the per-user `TMPDIR`
        /// environment variable among the default writable roots. Defaults to
        /// `false`.
        #[serde(default)]
        exclude_tmpdir_env_var: bool,

        /// When set to `true`, will NOT include the `/tmp` among the default
        /// writable roots on UNIX. Defaults to `false`.
        #[serde(default)]
        exclude_slash_tmp: bool,
    },
}

/// A writable root path accompanied by a list of subpaths that should remain
/// read-only even when the root is writable.
#[derive(Debug, Clone, PartialEq, Eq, JsonSchema)]
pub struct WritableRoot {
    pub root: AbsolutePathBuf,

    /// By construction, these subpaths are all under `root`.
    pub read_only_subpaths: Vec<AbsolutePathBuf>,
}

impl WritableRoot {
    pub fn is_path_writable(&self, path: &Path) -> bool {
        // Check if the path is under the root.
        if !path.starts_with(&self.root) {
            return false;
        }

        // Check if the path is under any of the read-only subpaths.
        for subpath in &self.read_only_subpaths {
            if path.starts_with(subpath) {
                return false;
            }
        }

        true
    }
}

impl FromStr for SandboxPolicy {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl FromStr for FileSystemSandboxPolicy {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl FromStr for NetworkSandboxPolicy {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        serde_json::from_str(s)
    }
}

impl SandboxPolicy {
    /// Returns a policy with read-only disk access and no network.
    pub fn new_read_only_policy() -> Self {
        SandboxPolicy::ReadOnly {
            access: ReadOnlyAccess::FullAccess,
            network_access: false,
        }
    }

    /// Returns a policy that can read the entire disk, but can only write to
    /// the current working directory and the per-user tmp dir on macOS. It does
    /// not allow network access.
    pub fn new_workspace_write_policy() -> Self {
        SandboxPolicy::WorkspaceWrite {
            writable_roots: vec![],
            read_only_access: ReadOnlyAccess::FullAccess,
            network_access: false,
            exclude_tmpdir_env_var: false,
            exclude_slash_tmp: false,
        }
    }

    pub fn has_full_disk_read_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ExternalSandbox { .. } => true,
            SandboxPolicy::ReadOnly { access, .. } => access.has_full_disk_read_access(),
            SandboxPolicy::WorkspaceWrite {
                read_only_access, ..
            } => read_only_access.has_full_disk_read_access(),
        }
    }

    pub fn has_full_disk_write_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ExternalSandbox { .. } => true,
            SandboxPolicy::ReadOnly { .. } => false,
            SandboxPolicy::WorkspaceWrite { .. } => false,
        }
    }

    pub fn has_full_network_access(&self) -> bool {
        match self {
            SandboxPolicy::DangerFullAccess => true,
            SandboxPolicy::ExternalSandbox { network_access } => network_access.is_enabled(),
            SandboxPolicy::ReadOnly { network_access, .. } => *network_access,
            SandboxPolicy::WorkspaceWrite { network_access, .. } => *network_access,
        }
    }

    /// Returns true if platform defaults should be included for restricted read access.
    pub fn include_platform_defaults(&self) -> bool {
        if self.has_full_disk_read_access() {
            return false;
        }
        match self {
            SandboxPolicy::ReadOnly { access, .. } => access.include_platform_defaults(),
            SandboxPolicy::WorkspaceWrite {
                read_only_access, ..
            } => read_only_access.include_platform_defaults(),
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. } => false,
        }
    }

    /// Returns the list of readable roots (tailored to the current working
    /// directory) when read access is restricted.
    ///
    /// For policies with full read access, this returns an empty list because
    /// callers should grant blanket reads.
    pub fn get_readable_roots_with_cwd(&self, cwd: &Path) -> Vec<AbsolutePathBuf> {
        let mut roots = match self {
            SandboxPolicy::DangerFullAccess | SandboxPolicy::ExternalSandbox { .. } => Vec::new(),
            SandboxPolicy::ReadOnly { access, .. } => access.get_readable_roots_with_cwd(cwd),
            SandboxPolicy::WorkspaceWrite {
                read_only_access, ..
            } => {
                let mut roots = read_only_access.get_readable_roots_with_cwd(cwd);
                roots.extend(
                    self.get_writable_roots_with_cwd(cwd)
                        .into_iter()
                        .map(|root| root.root),
                );
                roots
            }
        };
        let mut seen = HashSet::new();
        roots.retain(|root| seen.insert(root.to_path_buf()));
        roots
    }

    /// Returns the list of writable roots (tailored to the current working
    /// directory) together with subpaths that should remain read-only under
    /// each writable root.
    pub fn get_writable_roots_with_cwd(&self, cwd: &Path) -> Vec<WritableRoot> {
        match self {
            SandboxPolicy::DangerFullAccess => Vec::new(),
            SandboxPolicy::ExternalSandbox { .. } => Vec::new(),
            SandboxPolicy::ReadOnly { .. } => Vec::new(),
            SandboxPolicy::WorkspaceWrite {
                writable_roots,
                read_only_access: _,
                exclude_tmpdir_env_var,
                exclude_slash_tmp,
                network_access: _,
            } => {
                let mut roots: Vec<AbsolutePathBuf> = writable_roots.clone();

                let cwd_absolute = AbsolutePathBuf::from_absolute_path(cwd);
                match cwd_absolute {
                    Ok(cwd) => {
                        roots.push(cwd);
                    }
                    Err(e) => {
                        error!(
                            "Ignoring invalid cwd {:?} for sandbox writable root: {}",
                            cwd, e
                        );
                    }
                }

                if cfg!(unix) && !exclude_slash_tmp {
                    #[allow(clippy::expect_used)]
                    let slash_tmp =
                        AbsolutePathBuf::from_absolute_path("/tmp").expect("/tmp is absolute");
                    if slash_tmp.as_path().is_dir() {
                        roots.push(slash_tmp);
                    }
                }

                if !exclude_tmpdir_env_var
                    && let Some(tmpdir) = std::env::var_os("TMPDIR")
                    && !tmpdir.is_empty()
                {
                    match AbsolutePathBuf::from_absolute_path(PathBuf::from(&tmpdir)) {
                        Ok(tmpdir_path) => {
                            roots.push(tmpdir_path);
                        }
                        Err(e) => {
                            error!(
                                "Ignoring invalid TMPDIR value {tmpdir:?} for sandbox writable root: {e}",
                            );
                        }
                    }
                }

                let cwd_root = AbsolutePathBuf::from_absolute_path(cwd).ok();
                roots
                    .into_iter()
                    .map(|writable_root| {
                        let protect_missing_dot_codex = cwd_root
                            .as_ref()
                            .is_some_and(|cwd_root| cwd_root == &writable_root);
                        WritableRoot {
                            read_only_subpaths: default_read_only_subpaths_for_writable_root(
                                &writable_root,
                                protect_missing_dot_codex,
                            ),
                            root: writable_root,
                        }
                    })
                    .collect()
            }
        }
    }
}

fn default_read_only_subpaths_for_writable_root(
    writable_root: &AbsolutePathBuf,
    protect_missing_dot_codex: bool,
) -> Vec<AbsolutePathBuf> {
    let mut subpaths: Vec<AbsolutePathBuf> = Vec::new();
    #[allow(clippy::expect_used)]
    let top_level_git = writable_root
        .join(".git")
        .expect(".git is a valid relative path");
    let top_level_git_is_file = top_level_git.as_path().is_file();
    let top_level_git_is_dir = top_level_git.as_path().is_dir();
    if top_level_git_is_dir || top_level_git_is_file {
        if top_level_git_is_file
            && is_git_pointer_file(&top_level_git)
            && let Some(gitdir) = resolve_gitdir_from_file(&top_level_git)
        {
            subpaths.push(gitdir);
        }
        subpaths.push(top_level_git);
    }

    #[allow(clippy::expect_used)]
    let top_level_agents = writable_root.join(".agents").expect("valid relative path");
    if top_level_agents.as_path().is_dir() {
        subpaths.push(top_level_agents);
    }

    #[allow(clippy::expect_used)]
    let top_level_codex = writable_root.join(".codex").expect("valid relative path");
    if protect_missing_dot_codex || top_level_codex.as_path().is_dir() {
        subpaths.push(top_level_codex);
    }

    let mut deduped = Vec::with_capacity(subpaths.len());
    let mut seen = HashSet::new();
    for path in subpaths {
        if seen.insert(path.to_path_buf()) {
            deduped.push(path);
        }
    }
    deduped
}

fn is_git_pointer_file(path: &AbsolutePathBuf) -> bool {
    path.as_path().is_file() && path.as_path().file_name() == Some(OsStr::new(".git"))
}

fn resolve_gitdir_from_file(dot_git: &AbsolutePathBuf) -> Option<AbsolutePathBuf> {
    let contents = match std::fs::read_to_string(dot_git.as_path()) {
        Ok(contents) => contents,
        Err(err) => {
            error!(
                "Failed to read {path} for gitdir pointer: {err}",
                path = dot_git.as_path().display()
            );
            return None;
        }
    };

    let trimmed = contents.trim();
    let (_, gitdir_raw) = match trimmed.split_once(':') {
        Some(parts) => parts,
        None => {
            error!(
                "Expected {path} to contain a gitdir pointer, but it did not match `gitdir: <path>`.",
                path = dot_git.as_path().display()
            );
            return None;
        }
    };
    let gitdir_raw = gitdir_raw.trim();
    if gitdir_raw.is_empty() {
        error!(
            "Expected {path} to contain a gitdir pointer, but it was empty.",
            path = dot_git.as_path().display()
        );
        return None;
    }
    let base = match dot_git.as_path().parent() {
        Some(base) => base,
        None => {
            error!(
                "Unable to resolve parent directory for {path}.",
                path = dot_git.as_path().display()
            );
            return None;
        }
    };
    let gitdir_path = match AbsolutePathBuf::resolve_path_against_base(gitdir_raw, base) {
        Ok(path) => path,
        Err(err) => {
            error!(
                "Failed to resolve gitdir path {gitdir_raw} from {path}: {err}",
                path = dot_git.as_path().display()
            );
            return None;
        }
    };
    if !gitdir_path.as_path().exists() {
        error!(
            "Resolved gitdir path {path} does not exist.",
            path = gitdir_path.as_path().display()
        );
        return None;
    }
    Some(gitdir_path)
}
