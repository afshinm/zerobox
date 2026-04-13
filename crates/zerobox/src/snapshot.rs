//! Snapshot CLI integration.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use zerobox_snapshot::{
    Change, ChangeType, ExclusionConfig, ExclusionFilter, SessionMetadata, SnapshotManager,
    SnapshotManifest, WalkBudget, default_exclusions, dim, format_change_counts,
    format_change_counts_colored, format_relative_time, truncate_command,
};

use crate::{Cli, SnapshotAction};

pub struct SnapshotState {
    pub manager: SnapshotManager,
    pub session_id: String,
    pub tracked_paths: Vec<PathBuf>,
    pub exclude_patterns: Vec<String>,
    pub started: String,
}

pub fn build_snapshot_state(cli: &Cli, cwd: &Path) -> Result<SnapshotState> {
    let tracked_paths = resolve_tracked_paths(cli, cwd);

    let mut exclude_patterns = default_exclusions();
    if let Some(ref user_excludes) = cli.snapshot_exclude {
        exclude_patterns.extend(user_excludes.iter().cloned());
    }

    let config = ExclusionConfig {
        use_gitignore: true,
        exclude_patterns: exclude_patterns.clone(),
        exclude_globs: Vec::new(),
        force_include: Vec::new(),
    };
    let filter = ExclusionFilter::new(&config)?;

    let session_id = generate_session_id();
    let started = chrono::Utc::now().to_rfc3339();
    let session_dir = snapshots_dir().join(&session_id);

    let manager = SnapshotManager::new(
        session_dir,
        tracked_paths.clone(),
        filter,
        WalkBudget::default(),
    )?;

    Ok(SnapshotState {
        manager,
        session_id,
        tracked_paths,
        exclude_patterns,
        started,
    })
}

pub fn build_session_metadata(
    state: &SnapshotState,
    cli: &Cli,
    baseline: &SnapshotManifest,
) -> SessionMetadata {
    SessionMetadata {
        session_id: state.session_id.clone(),
        started: state.started.clone(),
        ended: None,
        command: cli.command.clone(),
        tracked_paths: state.tracked_paths.clone(),
        exclude_patterns: state.exclude_patterns.clone(),
        snapshot_count: 1,
        exit_code: None,
        merkle_roots: vec![baseline.merkle_root],
    }
}

pub fn print_summary_to(changes: &[Change], w: &mut dyn std::io::Write) {
    if changes.is_empty() {
        let _ = writeln!(w, "snapshot: no changes detected");
        return;
    }

    let mut created = 0usize;
    let mut modified = 0usize;
    let mut deleted = 0usize;
    let mut perms = 0usize;
    for c in changes {
        match c.change_type {
            ChangeType::Created => created += 1,
            ChangeType::Modified => modified += 1,
            ChangeType::Deleted => deleted += 1,
            ChangeType::PermissionsChanged => perms += 1,
        }
    }

    let _ = writeln!(
        w,
        "snapshot: {created} created, {modified} modified, {deleted} deleted{}",
        if perms > 0 {
            format!(", {perms} permissions changed")
        } else {
            String::new()
        }
    );

    for change in changes.iter().take(20) {
        let _ = writeln!(w, "  {} {}", change.change_type, change.path.display());
    }
    if changes.len() > 20 {
        let _ = writeln!(w, "  ... and {} more", changes.len() - 20);
    }
}

fn resolve_tracked_paths(cli: &Cli, cwd: &Path) -> Vec<PathBuf> {
    match &cli.snapshot_paths {
        Some(paths) => paths
            .iter()
            .map(|p| {
                let abs = if p.is_absolute() {
                    p.clone()
                } else {
                    cwd.join(p)
                };
                abs.canonicalize().unwrap_or(abs)
            })
            .collect(),
        None => vec![cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf())],
    }
}

fn generate_session_id() -> String {
    let now = chrono::Utc::now();
    format!("{}-{}", now.format("%Y%m%d-%H%M%S"), std::process::id())
}

fn snapshots_dir() -> PathBuf {
    zerobox::zerobox_home().join("snapshots")
}

fn validate_session_id(id: &str) -> bool {
    !id.is_empty()
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
        && id != "."
        && id != ".."
        && !id.contains("..")
}

pub fn handle_subcommand(action: &SnapshotAction) -> ExitCode {
    match action {
        SnapshotAction::List => cmd_list(),
        SnapshotAction::Diff { id } => cmd_diff(id),
        SnapshotAction::Restore { id } => cmd_restore(id),
        SnapshotAction::Clean { older_than } => cmd_clean(*older_than),
    }
}

fn cmd_list() -> ExitCode {
    let dir = snapshots_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("No snapshots found.");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: failed to read {}: {e}", dir.display());
            return ExitCode::from(1);
        }
    };

    let mut sessions: Vec<(String, SessionMetadata, Option<Vec<Change>>)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(meta) = SnapshotManager::load_session(&path) {
            let id = entry.file_name().to_string_lossy().to_string();
            let changes = if meta.snapshot_count >= 2 {
                let latest = meta.snapshot_count - 1;
                let changes_path = path.join(format!("changes/{latest:03}.json"));
                std::fs::read_to_string(&changes_path)
                    .ok()
                    .and_then(|json| serde_json::from_str::<Vec<Change>>(&json).ok())
            } else {
                None
            };
            sessions.push((id, meta, changes));
        }
    }

    if sessions.is_empty() {
        eprintln!("No snapshots found.");
        return ExitCode::SUCCESS;
    }

    sessions.sort_by(|a, b| b.1.started.cmp(&a.1.started));

    let use_color = std::io::IsTerminal::is_terminal(&std::io::stdout());
    let mut tw = tabwriter::TabWriter::new(std::io::stdout())
        .padding(2)
        .ansi(true);
    use std::io::Write;

    if use_color {
        let _ = writeln!(
            tw,
            "{}\t{}\t{}\t{}",
            dim("ID"),
            dim("TIME"),
            dim("COMMAND"),
            dim("CHANGES")
        );
    } else {
        let _ = writeln!(tw, "ID\tTIME\tCOMMAND\tCHANGES");
    }

    for (id, meta, changes) in &sessions {
        let time = format_relative_time(&meta.started);
        let cmd = truncate_command(&meta.command, 40);
        let changes_ref = changes.as_deref().unwrap_or(&[]);
        let summary = if use_color {
            format_change_counts_colored(changes_ref)
        } else {
            format_change_counts(changes_ref)
        };
        let _ = writeln!(tw, "{id}\t{time}\t{cmd}\t{summary}");
    }
    let _ = tw.flush();

    ExitCode::SUCCESS
}

fn cmd_diff(id: &str) -> ExitCode {
    if !validate_session_id(id) {
        eprintln!("error: invalid session ID: {id}");
        return ExitCode::from(1);
    }
    let session_dir = snapshots_dir().join(id);
    if !session_dir.exists() {
        eprintln!("error: session not found: {id}");
        return ExitCode::from(1);
    }

    let meta = match SnapshotManager::load_session(&session_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: failed to load session: {e}");
            return ExitCode::from(1);
        }
    };

    if meta.snapshot_count < 2 {
        eprintln!("No changes recorded for session {id}.");
        return ExitCode::SUCCESS;
    }
    let latest = meta.snapshot_count - 1;
    let changes_path = session_dir.join(format!("changes/{latest:03}.json"));
    if !changes_path.exists() {
        eprintln!("No changes recorded for session {id}.");
        return ExitCode::SUCCESS;
    }

    match std::fs::read_to_string(&changes_path) {
        Ok(json) => match serde_json::from_str::<Vec<Change>>(&json) {
            Ok(changes) => {
                print_summary_to(&changes, &mut std::io::stdout());
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("error: failed to parse changes: {e}");
                ExitCode::from(1)
            }
        },
        Err(e) => {
            eprintln!("error: failed to read changes: {e}");
            ExitCode::from(1)
        }
    }
}

fn cmd_restore(id: &str) -> ExitCode {
    if !validate_session_id(id) {
        eprintln!("error: invalid session ID: {id}");
        return ExitCode::from(1);
    }
    let session_dir = snapshots_dir().join(id);
    if !session_dir.exists() {
        eprintln!("error: session not found: {id}");
        return ExitCode::from(1);
    }

    let meta = match SnapshotManager::load_session(&session_dir) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: failed to load session: {e}");
            return ExitCode::from(1);
        }
    };

    let config = ExclusionConfig {
        use_gitignore: true,
        exclude_patterns: meta.exclude_patterns.clone(),
        exclude_globs: Vec::new(),
        force_include: Vec::new(),
    };
    let filter = match ExclusionFilter::new(&config) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(1);
        }
    };

    let mgr = match SnapshotManager::new(
        session_dir.clone(),
        meta.tracked_paths.clone(),
        filter,
        WalkBudget::default(),
    ) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("error: failed to open session: {e}");
            return ExitCode::from(1);
        }
    };

    let manifest_path = session_dir.join("snapshots/000.json");
    let manifest: SnapshotManifest = match std::fs::read_to_string(&manifest_path) {
        Ok(json) => match serde_json::from_str(&json) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("error: failed to parse baseline manifest: {e}");
                return ExitCode::from(1);
            }
        },
        Err(e) => {
            eprintln!("error: failed to read baseline manifest: {e}");
            return ExitCode::from(1);
        }
    };

    match mgr.restore_to(&manifest) {
        Ok(applied) => {
            eprintln!("Restored {} files to pre-execution state.", applied.len());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("error: restore failed: {e:#}");
            ExitCode::from(1)
        }
    }
}

fn cmd_clean(older_than_days: u64) -> ExitCode {
    let dir = snapshots_dir();
    let entries = match std::fs::read_dir(&dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("No snapshots found.");
            return ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("error: failed to read {}: {e}", dir.display());
            return ExitCode::from(1);
        }
    };

    let days = i64::try_from(older_than_days).unwrap_or(i64::MAX);
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days);
    let mut removed = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Ok(meta) = SnapshotManager::load_session(&path) {
            let started = chrono::DateTime::parse_from_rfc3339(&meta.started)
                .map(|dt| dt.with_timezone(&chrono::Utc));
            if let Ok(dt) = started
                && dt < cutoff
            {
                if let Err(e) = std::fs::remove_dir_all(&path) {
                    eprintln!("warning: failed to remove {}: {e}", path.display());
                } else {
                    removed += 1;
                }
            }
        }
    }

    eprintln!("Removed {removed} snapshot sessions.");
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_session_ids() {
        assert!(validate_session_id("20260403-120000-1234"));
        assert!(validate_session_id("my-session"));
        assert!(validate_session_id("abc123"));
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(!validate_session_id("../../../etc"));
        assert!(!validate_session_id("foo/bar"));
        assert!(!validate_session_id(".."));
        assert!(!validate_session_id("."));
        assert!(!validate_session_id(""));
        assert!(!validate_session_id("foo\0bar"));
        assert!(!validate_session_id("foo\\bar"));
    }

    #[test]
    fn zerobox_home_precedence() {
        let home = zerobox::zerobox_home();
        assert!(!home.as_os_str().is_empty());
        assert!(snapshots_dir().ends_with("snapshots"));
    }
}
