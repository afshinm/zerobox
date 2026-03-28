//! CLI flags → Codex sandbox policy types.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use codex_protocol::permissions::{
    FileSystemAccessMode, FileSystemPath, FileSystemSandboxEntry, FileSystemSandboxPolicy,
    FileSystemSpecialPath, NetworkSandboxPolicy,
};
use codex_protocol::protocol::SandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;

use crate::Cli;

/// Pre-resolved paths from CLI flags.
pub struct ResolvedPaths {
    pub readable: Option<Vec<AbsolutePathBuf>>,
    pub deny_readable: Vec<AbsolutePathBuf>,
    pub writable: Option<Vec<AbsolutePathBuf>>,
    pub deny_writable: Vec<AbsolutePathBuf>,
    pub full_write: bool,
}

pub fn resolve_path(base: &Path, p: &Path) -> Result<AbsolutePathBuf> {
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

pub fn resolve_cli_paths(cli: &Cli, cwd: &Path) -> Result<ResolvedPaths> {
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

pub fn build_fs_policy(
    resolved: &ResolvedPaths,
    allow_all: bool,
    net_enabled: bool,
) -> FileSystemSandboxPolicy {
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
            // On Linux, bubblewrap creates an isolated mount namespace. The
            // sandbox helper binary must be readable inside it for the seccomp
            // re-exec to work. Add the binary's own directory.
            if let Ok(exe) = std::env::current_exe()
                && let Some(dir) = exe.parent()
                && let Ok(abs) = AbsolutePathBuf::try_from(dir.to_path_buf())
            {
                entries.push(FileSystemSandboxEntry {
                    path: FileSystemPath::Path { path: abs },
                    access: FileSystemAccessMode::Read,
                });
            }
            // On systemd-based Linux, /etc/resolv.conf is a symlink to
            // /run/systemd/resolve/stub-resolv.conf. When network is enabled,
            // /run must be readable for DNS to work inside the bwrap namespace.
            if net_enabled && let Ok(abs) = AbsolutePathBuf::try_from(PathBuf::from("/run")) {
                entries.push(FileSystemSandboxEntry {
                    path: FileSystemPath::Path { path: abs },
                    access: FileSystemAccessMode::Read,
                });
            }
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

pub fn net_is_enabled(cli: &Cli) -> bool {
    cli.allow_all || cli.allow_net.is_some()
}

pub fn build_net_policy(cli: &Cli) -> NetworkSandboxPolicy {
    if net_is_enabled(cli) {
        NetworkSandboxPolicy::Enabled
    } else {
        NetworkSandboxPolicy::Restricted
    }
}

pub fn build_legacy_sandbox_policy(resolved: &ResolvedPaths, cli: &Cli) -> SandboxPolicy {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cli_defaults() -> Cli {
        Cli {
            allow_read: None,
            deny_read: None,
            allow_write: None,
            deny_write: None,
            allow_net: None,
            deny_net: None,
            allow_all: false,
            cwd: None,
            no_sandbox: false,
            command: vec!["true".to_string()],
        }
    }

    // ── resolve_path ──

    #[test]
    fn resolve_path_absolute_unchanged() {
        let result =
            resolve_path(Path::new("/base"), Path::new("/absolute/path")).expect("resolve");
        assert_eq!(result.as_path(), Path::new("/absolute/path"));
    }

    #[test]
    fn resolve_path_relative_joined_to_base() {
        let result = resolve_path(Path::new("/base/dir"), Path::new("child")).expect("resolve");
        assert_eq!(result.as_path(), Path::new("/base/dir/child"));
    }

    // ── resolve_cli_paths ──

    #[test]
    fn resolve_defaults_all_empty() {
        let cli = cli_defaults();
        let resolved = resolve_cli_paths(&cli, Path::new("/tmp")).expect("resolve");
        assert!(resolved.readable.is_none());
        assert!(resolved.deny_readable.is_empty());
        assert!(resolved.writable.is_none());
        assert!(resolved.deny_writable.is_empty());
        assert!(!resolved.full_write);
    }

    #[test]
    fn resolve_allow_write_empty_means_full_write() {
        let mut cli = cli_defaults();
        cli.allow_write = Some(vec![]);
        let resolved = resolve_cli_paths(&cli, Path::new("/tmp")).expect("resolve");
        assert!(resolved.full_write);
        assert!(resolved.writable.is_none());
    }

    #[test]
    fn resolve_allow_write_with_paths() {
        let mut cli = cli_defaults();
        cli.allow_write = Some(vec![PathBuf::from("/tmp")]);
        let resolved = resolve_cli_paths(&cli, Path::new("/")).expect("resolve");
        assert!(!resolved.full_write);
        assert_eq!(resolved.writable.as_ref().map(|v| v.len()), Some(1));
    }

    #[test]
    fn resolve_deny_paths() {
        let mut cli = cli_defaults();
        cli.deny_read = Some(vec![PathBuf::from("/secret")]);
        cli.deny_write = Some(vec![PathBuf::from("/protected")]);
        let resolved = resolve_cli_paths(&cli, Path::new("/")).expect("resolve");
        assert_eq!(resolved.deny_readable.len(), 1);
        assert_eq!(resolved.deny_writable.len(), 1);
    }

    // ── build_fs_policy ──

    #[test]
    fn fs_policy_allow_all_is_unrestricted() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: false,
        };
        assert_eq!(
            build_fs_policy(&resolved, true, false),
            FileSystemSandboxPolicy::unrestricted()
        );
    }

    #[test]
    fn fs_policy_default_has_full_disk_read() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: false,
        };
        let policy = build_fs_policy(&resolved, false, false);
        assert!(policy.has_full_disk_read_access());
        assert!(!policy.has_full_disk_write_access());
    }

    #[test]
    fn fs_policy_allow_read_includes_minimal() {
        let resolved = ResolvedPaths {
            readable: Some(vec![
                AbsolutePathBuf::try_from(PathBuf::from("/tmp")).expect("abs"),
            ]),
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: false,
        };
        let policy = build_fs_policy(&resolved, false, false);
        assert!(!policy.has_full_disk_read_access());
        assert!(policy.include_platform_defaults());
    }

    #[test]
    fn fs_policy_full_write() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: true,
        };
        let policy = build_fs_policy(&resolved, false, false);
        assert!(policy.has_full_disk_write_access());
    }

    // ── build_legacy_sandbox_policy ──

    #[test]
    fn legacy_allow_all_is_danger() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: false,
        };
        let mut cli = cli_defaults();
        cli.allow_all = true;
        assert!(matches!(
            build_legacy_sandbox_policy(&resolved, &cli),
            SandboxPolicy::DangerFullAccess
        ));
    }

    #[test]
    fn legacy_full_write_is_danger() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: true,
        };
        let cli = cli_defaults();
        assert!(matches!(
            build_legacy_sandbox_policy(&resolved, &cli),
            SandboxPolicy::DangerFullAccess
        ));
    }

    #[test]
    fn legacy_writable_roots_is_workspace_write() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: Some(vec![
                AbsolutePathBuf::try_from(PathBuf::from("/tmp")).expect("abs"),
            ]),
            deny_writable: vec![],
            full_write: false,
        };
        let cli = cli_defaults();
        assert!(matches!(
            build_legacy_sandbox_policy(&resolved, &cli),
            SandboxPolicy::WorkspaceWrite { .. }
        ));
    }

    #[test]
    fn legacy_default_is_read_only() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: false,
        };
        let cli = cli_defaults();
        assert!(matches!(
            build_legacy_sandbox_policy(&resolved, &cli),
            SandboxPolicy::ReadOnly {
                network_access: false,
                ..
            }
        ));
    }

    #[test]
    fn legacy_read_only_with_network() {
        let resolved = ResolvedPaths {
            readable: None,
            deny_readable: vec![],
            writable: None,
            deny_writable: vec![],
            full_write: false,
        };
        let mut cli = cli_defaults();
        cli.allow_net = Some(vec![]);
        assert!(matches!(
            build_legacy_sandbox_policy(&resolved, &cli),
            SandboxPolicy::ReadOnly {
                network_access: true,
                ..
            }
        ));
    }

    // ── net_is_enabled / build_net_policy ──

    #[test]
    fn net_disabled_by_default() {
        let cli = cli_defaults();
        assert!(!net_is_enabled(&cli));
        assert_eq!(build_net_policy(&cli), NetworkSandboxPolicy::Restricted);
    }

    #[test]
    fn net_enabled_with_allow_net() {
        let mut cli = cli_defaults();
        cli.allow_net = Some(vec![]);
        assert!(net_is_enabled(&cli));
        assert_eq!(build_net_policy(&cli), NetworkSandboxPolicy::Enabled);
    }

    #[test]
    fn net_enabled_with_allow_all() {
        let mut cli = cli_defaults();
        cli.allow_all = true;
        assert!(net_is_enabled(&cli));
        assert_eq!(build_net_policy(&cli), NetworkSandboxPolicy::Enabled);
    }

    #[test]
    fn net_enabled_with_domain_filter() {
        let mut cli = cli_defaults();
        cli.allow_net = Some(vec!["example.com".to_string()]);
        assert!(net_is_enabled(&cli));
    }
}
