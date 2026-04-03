//! Snapshot lifecycle: baseline capture, incremental diffing, and restore.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::budget::WalkBudget;
use crate::exclusion::ExclusionFilter;
use crate::merkle::merkle_root;
use crate::store::ObjectStore;
use crate::types::*;

pub struct SnapshotManager {
    session_dir: PathBuf,
    tracked_paths: Vec<PathBuf>,
    exclusion: ExclusionFilter,
    store: ObjectStore,
    budget: WalkBudget,
    snapshot_count: u32,
}

impl SnapshotManager {
    pub fn new(
        session_dir: PathBuf,
        tracked_paths: Vec<PathBuf>,
        exclusion: ExclusionFilter,
        budget: WalkBudget,
    ) -> Result<Self> {
        std::fs::create_dir_all(session_dir.join("snapshots"))?;
        std::fs::create_dir_all(session_dir.join("changes"))?;
        let store = ObjectStore::new(session_dir.join("cache"))?;
        Ok(Self {
            session_dir,
            tracked_paths,
            exclusion,
            store,
            budget,
            snapshot_count: 0,
        })
    }

    /// Capture the initial state of all tracked paths (snapshot 0).
    pub fn create_baseline(&mut self) -> Result<SnapshotManifest> {
        let files = self.walk_and_store()?;
        let root = merkle_root(&files)?;
        let manifest = SnapshotManifest {
            number: 0,
            timestamp: now_epoch_secs(),
            parent: None,
            files,
            merkle_root: root,
        };
        self.save_manifest(&manifest)?;
        self.snapshot_count = 1;
        Ok(manifest)
    }

    /// Snapshot again and diff against the previous manifest.
    pub fn create_incremental(
        &mut self,
        previous: &SnapshotManifest,
    ) -> Result<(SnapshotManifest, Vec<Change>)> {
        let current_files = self.walk_and_store()?;
        let changes = compute_changes(&previous.files, &current_files);
        let root = merkle_root(&current_files)?;
        let number = previous.number + 1;

        let manifest = SnapshotManifest {
            number,
            timestamp: now_epoch_secs(),
            parent: Some(previous.number),
            files: current_files,
            merkle_root: root,
        };

        self.save_manifest(&manifest)?;
        if !changes.is_empty() {
            let changes_json = serde_json::to_string_pretty(&changes)?;
            atomic_write(
                &self.session_dir.join(format!("changes/{number:03}.json")),
                changes_json.as_bytes(),
            )?;
        }
        self.snapshot_count = number + 1;
        Ok((manifest, changes))
    }

    /// Restore the filesystem to match a manifest. Returns applied changes.
    pub fn restore_to(&self, manifest: &SnapshotManifest) -> Result<Vec<Change>> {
        self.validate_manifest_paths(manifest)?;
        let current_files = self.walk_current()?;
        let mut applied = Vec::new();

        for (path, state) in &manifest.files {
            let needs_restore = match current_files.get(path) {
                Some(current) => current.hash != state.hash,
                None => true,
            };
            if needs_restore {
                self.store.retrieve_to(&state.hash, path)?;

                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(state.permissions & 0o0777);
                    if let Err(e) = std::fs::set_permissions(path, perms) {
                        tracing::warn!("failed to set permissions on {}: {e}", path.display());
                    }
                }

                let change_type = if current_files.contains_key(path) {
                    ChangeType::Modified
                } else {
                    ChangeType::Created
                };
                applied.push(Change {
                    path: path.clone(),
                    change_type,
                    size_delta: None,
                    old_hash: current_files.get(path).map(|s| s.hash),
                    new_hash: Some(state.hash),
                });
            }
        }

        for path in current_files.keys() {
            if !manifest.files.contains_key(path) {
                if let Err(e) = std::fs::remove_file(path) {
                    tracing::warn!("failed to delete {}: {e}", path.display());
                } else {
                    applied.push(Change {
                        path: path.clone(),
                        change_type: ChangeType::Deleted,
                        size_delta: None,
                        old_hash: current_files.get(path).map(|s| s.hash),
                        new_hash: None,
                    });
                }
            }
        }

        Ok(applied)
    }

    /// Dry-run: show what restore_to would change without touching disk.
    pub fn compute_restore_diff(&self, manifest: &SnapshotManifest) -> Result<Vec<Change>> {
        self.validate_manifest_paths(manifest)?;
        let current_files = self.walk_current()?;
        let mut changes = Vec::new();

        for (path, state) in &manifest.files {
            match current_files.get(path) {
                Some(current) if current.hash != state.hash => {
                    changes.push(Change {
                        path: path.clone(),
                        change_type: ChangeType::Modified,
                        size_delta: Some(state.size as i64 - current.size as i64),
                        old_hash: Some(current.hash),
                        new_hash: Some(state.hash),
                    });
                }
                None => {
                    changes.push(Change {
                        path: path.clone(),
                        change_type: ChangeType::Created,
                        size_delta: Some(state.size as i64),
                        old_hash: None,
                        new_hash: Some(state.hash),
                    });
                }
                _ => {}
            }
        }

        for (path, state) in &current_files {
            if !manifest.files.contains_key(path) {
                changes.push(Change {
                    path: path.clone(),
                    change_type: ChangeType::Deleted,
                    size_delta: Some(-(state.size as i64)),
                    old_hash: Some(state.hash),
                    new_hash: None,
                });
            }
        }

        changes.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(changes)
    }

    pub fn save_session(&self, meta: &SessionMetadata) -> Result<()> {
        let json = serde_json::to_string_pretty(meta)?;
        atomic_write(&self.session_dir.join("session.json"), json.as_bytes())
    }

    pub fn load_session(session_dir: &Path) -> Result<SessionMetadata> {
        let json = std::fs::read_to_string(session_dir.join("session.json"))
            .context("failed to read session.json")?;
        Ok(serde_json::from_str(&json)?)
    }

    pub fn snapshot_count(&self) -> u32 {
        self.snapshot_count
    }

    fn walk_and_store(&self) -> Result<BTreeMap<PathBuf, FileState>> {
        self.walk_files(true)
    }

    fn walk_current(&self) -> Result<BTreeMap<PathBuf, FileState>> {
        self.walk_files(false)
    }

    /// Walk tracked paths with exclusion filtering and budget enforcement.
    /// When `store` is true, file content is persisted to the object store.
    fn walk_files(&self, store: bool) -> Result<BTreeMap<PathBuf, FileState>> {
        let mut files = BTreeMap::new();
        let mut entries_visited: usize = 0;
        let mut total_bytes: u64 = 0;

        for tracked in &self.tracked_paths {
            if !tracked.exists() {
                continue;
            }

            if tracked.is_file() {
                if self.exclusion.is_excluded(tracked) {
                    continue;
                }
                let state = self.file_state(tracked, store)?;
                total_bytes += state.size;
                entries_visited += 1;
                self.budget.check(entries_visited, total_bytes)?;
                files.insert(tracked.clone(), state);
                continue;
            }

            let exclusion = self.exclusion.clone();
            let mut walker = ignore::WalkBuilder::new(tracked);
            walker.git_ignore(self.exclusion.use_gitignore());
            walker.hidden(false);
            walker.filter_entry(move |entry| !exclusion.is_excluded(entry.path()));

            for entry in walker.build() {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("walk error: {e}");
                        continue;
                    }
                };

                let path = entry.path();
                if !path.is_file() {
                    continue;
                }

                match self.file_state(path, store) {
                    Ok(state) => {
                        total_bytes += state.size;
                        entries_visited += 1;
                        self.budget.check(entries_visited, total_bytes)?;
                        files.insert(path.to_path_buf(), state);
                    }
                    Err(e) => {
                        tracing::warn!("failed to process {}: {e}", path.display());
                    }
                }
            }
        }

        Ok(files)
    }

    /// Read file metadata and content hash. Persists to the object store when `store` is true.
    fn file_state(&self, path: &Path, store: bool) -> Result<FileState> {
        let meta = std::fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?;
        let hash = if store {
            self.store.store_file(path)?
        } else {
            stream_hash(path)?
        };
        Ok(FileState {
            hash,
            size: meta.len(),
            mtime: file_mtime(&meta),
            permissions: file_permissions(&meta),
        })
    }

    fn save_manifest(&self, manifest: &SnapshotManifest) -> Result<()> {
        let json = serde_json::to_string_pretty(manifest)?;
        atomic_write(
            &self
                .session_dir
                .join(format!("snapshots/{:03}.json", manifest.number)),
            json.as_bytes(),
        )
    }

    /// Reject manifests with `..` traversal or paths outside tracked roots.
    fn validate_manifest_paths(&self, manifest: &SnapshotManifest) -> Result<()> {
        for path in manifest.files.keys() {
            for component in path.components() {
                if matches!(component, Component::ParentDir) {
                    bail!(
                        "manifest contains path with parent directory traversal: {}",
                        path.display()
                    );
                }
            }
            let within_tracked = self.tracked_paths.iter().any(|root| path.starts_with(root));
            if !within_tracked {
                bail!(
                    "manifest contains path outside tracked directories: {}",
                    path.display()
                );
            }
        }
        Ok(())
    }
}

/// Diff two file manifests into a sorted list of changes.
pub fn compute_changes(
    previous: &BTreeMap<PathBuf, FileState>,
    current: &BTreeMap<PathBuf, FileState>,
) -> Vec<Change> {
    let mut changes = Vec::new();

    for (path, prev_state) in previous {
        match current.get(path) {
            Some(cur_state) if cur_state.hash != prev_state.hash => {
                changes.push(Change {
                    path: path.clone(),
                    change_type: ChangeType::Modified,
                    size_delta: Some(cur_state.size as i64 - prev_state.size as i64),
                    old_hash: Some(prev_state.hash),
                    new_hash: Some(cur_state.hash),
                });
            }
            Some(cur_state) if cur_state.permissions != prev_state.permissions => {
                changes.push(Change {
                    path: path.clone(),
                    change_type: ChangeType::PermissionsChanged,
                    size_delta: Some(0),
                    old_hash: Some(prev_state.hash),
                    new_hash: Some(cur_state.hash),
                });
            }
            None => {
                changes.push(Change {
                    path: path.clone(),
                    change_type: ChangeType::Deleted,
                    size_delta: Some(-(prev_state.size as i64)),
                    old_hash: Some(prev_state.hash),
                    new_hash: None,
                });
            }
            _ => {}
        }
    }

    for (path, cur_state) in current {
        if !previous.contains_key(path) {
            changes.push(Change {
                path: path.clone(),
                change_type: ChangeType::Created,
                size_delta: Some(cur_state.size as i64),
                old_hash: None,
                new_hash: Some(cur_state.hash),
            });
        }
    }

    changes.sort_by(|a, b| a.path.cmp(&b.path));
    changes
}

/// Atomic write via temp file + rename in the same directory.
fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(parent)?;

    let mut tmp = tempfile::NamedTempFile::new_in(parent)
        .with_context(|| format!("failed to create temp file in {}", parent.display()))?;
    std::io::Write::write_all(&mut tmp, content)?;
    tmp.persist(path)
        .map_err(|e| anyhow::anyhow!("failed to persist {}: {}", path.display(), e.error))?;
    Ok(())
}

/// Hash a file with BLAKE3 using streaming I/O (constant memory).
fn stream_hash(path: &Path) -> Result<ContentHash> {
    let mut file =
        std::fs::File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = std::io::Read::read(&mut file, &mut buf)
            .with_context(|| format!("failed to read {}", path.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(ContentHash::from_bytes(*hasher.finalize().as_bytes()))
}

fn now_epoch_secs() -> String {
    chrono::Utc::now().timestamp().to_string()
}

fn file_mtime(meta: &std::fs::Metadata) -> i64 {
    filetime::FileTime::from_last_modification_time(meta).unix_seconds()
}

#[cfg(unix)]
fn file_permissions(meta: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt;
    meta.permissions().mode()
}

#[cfg(not(unix))]
fn file_permissions(meta: &std::fs::Metadata) -> u32 {
    if meta.permissions().readonly() {
        0o444
    } else {
        0o644
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::budget::WalkBudget;
    use crate::exclusion::{ExclusionConfig, ExclusionFilter};

    fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("file1.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("file2.txt"), "world").unwrap();
        dir
    }

    fn make_manager(dir: &Path, tracked: &Path) -> SnapshotManager {
        let config = ExclusionConfig {
            use_gitignore: false,
            ..ExclusionConfig::default()
        };
        let filter = ExclusionFilter::new(&config).unwrap();
        SnapshotManager::new(
            dir.join("session"),
            vec![tracked.to_path_buf()],
            filter,
            WalkBudget::default(),
        )
        .unwrap()
    }

    #[test]
    fn baseline_captures_all_files() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();
        assert_eq!(baseline.number, 0);
        assert!(baseline.parent.is_none());
        assert_eq!(baseline.files.len(), 2);
    }

    #[test]
    fn incremental_detects_modification() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("file1.txt"), "changed").unwrap();
        let (manifest, changes) = mgr.create_incremental(&baseline).unwrap();
        assert_eq!(manifest.number, 1);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn incremental_detects_creation() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("new.txt"), "new file").unwrap();
        let (_, changes) = mgr.create_incremental(&baseline).unwrap();
        assert!(changes.iter().any(|c| c.change_type == ChangeType::Created));
    }

    #[test]
    fn incremental_detects_deletion() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::remove_file(test_dir.path().join("file1.txt")).unwrap();
        let (_, changes) = mgr.create_incremental(&baseline).unwrap();
        assert!(changes.iter().any(|c| c.change_type == ChangeType::Deleted));
    }

    #[test]
    fn restore_reverts_to_baseline() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("file1.txt"), "changed").unwrap();
        std::fs::write(test_dir.path().join("new.txt"), "extra").unwrap();
        std::fs::remove_file(test_dir.path().join("file2.txt")).unwrap();

        let applied = mgr.restore_to(&baseline).unwrap();
        assert!(!applied.is_empty());
        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("file1.txt")).unwrap(),
            "hello"
        );
        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("file2.txt")).unwrap(),
            "world"
        );
        assert!(!test_dir.path().join("new.txt").exists());
    }

    #[test]
    fn merkle_root_differs_between_snapshots() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("file1.txt"), "different").unwrap();
        let (incremental, _) = mgr.create_incremental(&baseline).unwrap();
        assert_ne!(baseline.merkle_root, incremental.merkle_root);
    }

    #[test]
    fn compute_restore_diff_is_readonly() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("file1.txt"), "changed").unwrap();
        let diff = mgr.compute_restore_diff(&baseline).unwrap();
        assert!(!diff.is_empty());

        // Diff is read-only: file should still be "changed".
        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("file1.txt")).unwrap(),
            "changed"
        );
    }

    #[test]
    fn validate_rejects_parent_dir_traversal() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mgr = make_manager(session_dir.path(), test_dir.path());

        let mut files = BTreeMap::new();
        files.insert(
            test_dir.path().join("../../../etc/passwd"),
            FileState {
                hash: ContentHash::from_bytes([0; 32]),
                size: 0,
                mtime: 0,
                permissions: 0o644,
            },
        );
        let manifest = SnapshotManifest {
            number: 0,
            timestamp: "0".to_string(),
            parent: None,
            files,
            merkle_root: ContentHash::from_bytes([0; 32]),
        };
        assert!(mgr.restore_to(&manifest).is_err());
    }

    #[test]
    fn validate_rejects_path_outside_tracked() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mgr = make_manager(session_dir.path(), test_dir.path());

        let mut files = BTreeMap::new();
        files.insert(
            PathBuf::from("/tmp/unrelated/file.txt"),
            FileState {
                hash: ContentHash::from_bytes([0; 32]),
                size: 0,
                mtime: 0,
                permissions: 0o644,
            },
        );
        let manifest = SnapshotManifest {
            number: 0,
            timestamp: "0".to_string(),
            parent: None,
            files,
            merkle_root: ContentHash::from_bytes([0; 32]),
        };
        assert!(mgr.restore_to(&manifest).is_err());
    }

    #[test]
    fn session_metadata_roundtrip() {
        let session_dir = tempfile::tempdir().unwrap();
        let meta = SessionMetadata {
            session_id: "20260402-120000-1234".to_string(),
            started: "2026-04-02T12:00:00Z".to_string(),
            ended: Some("2026-04-02T12:00:10Z".to_string()),
            command: vec!["echo".to_string(), "hello".to_string()],
            tracked_paths: vec![PathBuf::from("/tmp/test")],
            exclude_patterns: vec![".git".to_string()],
            snapshot_count: 2,
            exit_code: Some(0),
            merkle_roots: vec![],
        };

        let filter = ExclusionFilter::new(&ExclusionConfig::default()).unwrap();
        let mgr = SnapshotManager::new(
            session_dir.path().join("session"),
            vec![],
            filter,
            WalkBudget::default(),
        )
        .unwrap();
        mgr.save_session(&meta).unwrap();

        let loaded = SnapshotManager::load_session(&session_dir.path().join("session")).unwrap();
        assert_eq!(loaded.session_id, meta.session_id);
        assert_eq!(loaded.exit_code, Some(0));
    }

    #[test]
    fn walk_budget_entry_limit() {
        let test_dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            std::fs::write(test_dir.path().join(format!("f{i}.txt")), "x").unwrap();
        }
        let session_dir = tempfile::tempdir().unwrap();
        let filter = ExclusionFilter::new(&ExclusionConfig::default()).unwrap();
        let mut mgr = SnapshotManager::new(
            session_dir.path().join("session"),
            vec![test_dir.path().to_path_buf()],
            filter,
            WalkBudget {
                max_entries: 5,
                max_bytes: 0,
            },
        )
        .unwrap();
        assert!(mgr.create_baseline().is_err());
    }

    #[test]
    fn exclusion_filter_omits_files_from_snapshot() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::write(test_dir.path().join("keep.txt"), "yes").unwrap();
        std::fs::create_dir_all(test_dir.path().join("node_modules")).unwrap();
        std::fs::write(test_dir.path().join("node_modules/pkg.js"), "no").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let config = ExclusionConfig {
            use_gitignore: false,
            exclude_patterns: vec!["node_modules".to_string()],
            exclude_globs: Vec::new(),
            force_include: Vec::new(),
        };
        let filter = ExclusionFilter::new(&config).unwrap();
        let mut mgr = SnapshotManager::new(
            session_dir.path().join("session"),
            vec![test_dir.path().to_path_buf()],
            filter,
            WalkBudget::default(),
        )
        .unwrap();

        let baseline = mgr.create_baseline().unwrap();
        assert_eq!(baseline.files.len(), 1);
        assert!(baseline.files.keys().any(|p| p.ends_with("keep.txt")));
    }

    #[test]
    fn restore_returns_correct_change_types() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("file1.txt"), "changed").unwrap();
        std::fs::write(test_dir.path().join("new.txt"), "extra").unwrap();
        std::fs::remove_file(test_dir.path().join("file2.txt")).unwrap();

        let applied = mgr.restore_to(&baseline).unwrap();

        let modified = applied
            .iter()
            .filter(|c| c.change_type == ChangeType::Modified)
            .count();
        let created = applied
            .iter()
            .filter(|c| c.change_type == ChangeType::Created)
            .count();
        let deleted = applied
            .iter()
            .filter(|c| c.change_type == ChangeType::Deleted)
            .count();

        assert_eq!(modified, 1); // file1.txt reverted
        assert_eq!(created, 1); // file2.txt recreated
        assert_eq!(deleted, 1); // new.txt removed
    }

    #[test]
    fn permissions_change_detected() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o755);
            std::fs::set_permissions(test_dir.path().join("file1.txt"), perms).unwrap();
        }

        let (_, changes) = mgr.create_incremental(&baseline).unwrap();
        #[cfg(unix)]
        assert!(
            changes
                .iter()
                .any(|c| c.change_type == ChangeType::PermissionsChanged)
        );
    }

    fn make_manager_with_exclusions(
        session_dir: &Path,
        tracked: &Path,
        exclude: Vec<&str>,
    ) -> SnapshotManager {
        let config = ExclusionConfig {
            use_gitignore: false,
            exclude_patterns: exclude.into_iter().map(String::from).collect(),
            exclude_globs: Vec::new(),
            force_include: Vec::new(),
        };
        let filter = ExclusionFilter::new(&config).unwrap();
        SnapshotManager::new(
            session_dir.join("session"),
            vec![tracked.to_path_buf()],
            filter,
            WalkBudget::default(),
        )
        .unwrap()
    }

    #[test]
    fn restore_does_not_delete_excluded_git_dir() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::write(test_dir.path().join("file.txt"), "tracked").unwrap();
        std::fs::create_dir_all(test_dir.path().join(".git/objects")).unwrap();
        std::fs::write(test_dir.path().join(".git/HEAD"), "ref: refs/heads/main").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr =
            make_manager_with_exclusions(session_dir.path(), test_dir.path(), vec![".git"]);

        let baseline = mgr.create_baseline().unwrap();
        std::fs::write(test_dir.path().join("file.txt"), "modified").unwrap();
        mgr.restore_to(&baseline).unwrap();

        assert!(test_dir.path().join(".git/HEAD").exists());
        assert!(test_dir.path().join(".git/objects").exists());
    }

    #[test]
    fn restore_does_not_delete_excluded_node_modules() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::write(test_dir.path().join("index.js"), "app").unwrap();
        std::fs::create_dir_all(test_dir.path().join("node_modules/lodash")).unwrap();
        std::fs::write(
            test_dir.path().join("node_modules/lodash/index.js"),
            "module",
        )
        .unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr =
            make_manager_with_exclusions(session_dir.path(), test_dir.path(), vec!["node_modules"]);

        let baseline = mgr.create_baseline().unwrap();
        assert_eq!(baseline.files.len(), 1);

        std::fs::write(test_dir.path().join("index.js"), "changed").unwrap();
        mgr.restore_to(&baseline).unwrap();

        assert!(
            test_dir
                .path()
                .join("node_modules/lodash/index.js")
                .exists()
        );
        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("index.js")).unwrap(),
            "app"
        );
    }

    #[test]
    fn restore_does_not_delete_multiple_excluded_dirs() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::write(test_dir.path().join("src.rs"), "code").unwrap();
        std::fs::create_dir_all(test_dir.path().join(".git")).unwrap();
        std::fs::write(test_dir.path().join(".git/config"), "git").unwrap();
        std::fs::create_dir_all(test_dir.path().join("target/debug")).unwrap();
        std::fs::write(test_dir.path().join("target/debug/bin"), "binary").unwrap();
        std::fs::create_dir_all(test_dir.path().join("node_modules/pkg")).unwrap();
        std::fs::write(test_dir.path().join("node_modules/pkg/lib.js"), "lib").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager_with_exclusions(
            session_dir.path(),
            test_dir.path(),
            vec![".git", "target", "node_modules"],
        );

        let baseline = mgr.create_baseline().unwrap();
        assert_eq!(baseline.files.len(), 1);

        std::fs::write(test_dir.path().join("src.rs"), "changed").unwrap();
        mgr.restore_to(&baseline).unwrap();

        assert!(test_dir.path().join(".git/config").exists());
        assert!(test_dir.path().join("target/debug/bin").exists());
        assert!(test_dir.path().join("node_modules/pkg/lib.js").exists());
        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("src.rs")).unwrap(),
            "code"
        );
    }

    #[test]
    fn restore_deletes_new_files_but_not_excluded_ones() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::write(test_dir.path().join("original.txt"), "keep").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr =
            make_manager_with_exclusions(session_dir.path(), test_dir.path(), vec![".git"]);

        let baseline = mgr.create_baseline().unwrap();

        // Create a new tracked file AND a new excluded dir after baseline.
        std::fs::write(test_dir.path().join("new_tracked.txt"), "delete me").unwrap();
        std::fs::create_dir_all(test_dir.path().join(".git")).unwrap();
        std::fs::write(test_dir.path().join(".git/HEAD"), "new git").unwrap();

        mgr.restore_to(&baseline).unwrap();

        // new_tracked.txt should be deleted (it was in scope but not in baseline).
        assert!(!test_dir.path().join("new_tracked.txt").exists());
        // .git should survive (excluded from both baseline and restore walk).
        assert!(test_dir.path().join(".git/HEAD").exists());
        // original.txt untouched.
        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("original.txt")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn restore_with_nested_excluded_dir_inside_tracked() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(test_dir.path().join("project/src")).unwrap();
        std::fs::write(test_dir.path().join("project/src/main.rs"), "fn main()").unwrap();
        std::fs::create_dir_all(test_dir.path().join("project/.git/refs")).unwrap();
        std::fs::write(test_dir.path().join("project/.git/HEAD"), "ref").unwrap();
        std::fs::write(test_dir.path().join("project/.git/refs/heads"), "sha").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager_with_exclusions(
            session_dir.path(),
            test_dir.path().join("project").as_path(),
            vec![".git"],
        );

        let baseline = mgr.create_baseline().unwrap();
        assert_eq!(baseline.files.len(), 1);

        // Modify tracked file and add a file inside .git.
        std::fs::write(test_dir.path().join("project/src/main.rs"), "changed").unwrap();
        std::fs::write(test_dir.path().join("project/.git/index"), "new index").unwrap();

        mgr.restore_to(&baseline).unwrap();

        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("project/src/main.rs")).unwrap(),
            "fn main()"
        );
        assert!(test_dir.path().join("project/.git/HEAD").exists());
        assert!(test_dir.path().join("project/.git/refs/heads").exists());
        assert!(test_dir.path().join("project/.git/index").exists());
    }

    #[test]
    fn diff_does_not_report_excluded_files_as_deletions() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::write(test_dir.path().join("file.txt"), "tracked").unwrap();
        std::fs::create_dir_all(test_dir.path().join(".git")).unwrap();
        std::fs::write(test_dir.path().join(".git/HEAD"), "ref").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr =
            make_manager_with_exclusions(session_dir.path(), test_dir.path(), vec![".git"]);

        let baseline = mgr.create_baseline().unwrap();
        let diff = mgr.compute_restore_diff(&baseline).unwrap();
        assert!(diff.is_empty());
    }

    #[test]
    fn empty_directory_produces_empty_baseline() {
        let test_dir = tempfile::tempdir().unwrap();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();
        assert!(baseline.files.is_empty());
    }

    #[test]
    fn nonexistent_tracked_path_produces_empty_baseline() {
        let session_dir = tempfile::tempdir().unwrap();
        let filter = ExclusionFilter::new(&ExclusionConfig::default()).unwrap();
        let mut mgr = SnapshotManager::new(
            session_dir.path().join("session"),
            vec![PathBuf::from("/nonexistent/path/that/does/not/exist")],
            filter,
            WalkBudget::default(),
        )
        .unwrap();
        let baseline = mgr.create_baseline().unwrap();
        assert!(baseline.files.is_empty());
    }

    #[test]
    fn tracked_single_file() {
        let test_dir = tempfile::tempdir().unwrap();
        let file_path = test_dir.path().join("only.txt");
        std::fs::write(&file_path, "single").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let filter = ExclusionFilter::new(&ExclusionConfig::default()).unwrap();
        let mut mgr = SnapshotManager::new(
            session_dir.path().join("session"),
            vec![file_path.clone()],
            filter,
            WalkBudget::default(),
        )
        .unwrap();

        let baseline = mgr.create_baseline().unwrap();
        assert_eq!(baseline.files.len(), 1);
        assert!(baseline.files.contains_key(&file_path));

        std::fs::write(&file_path, "changed").unwrap();
        let (_, changes) = mgr.create_incremental(&baseline).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].change_type, ChangeType::Modified);
    }

    #[test]
    fn restore_when_tracked_dir_deleted() {
        let test_dir = tempfile::tempdir().unwrap();
        std::fs::write(test_dir.path().join("file.txt"), "original").unwrap();

        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::remove_dir_all(test_dir.path()).unwrap();
        let applied = mgr.restore_to(&baseline).unwrap();
        assert_eq!(applied.len(), 1);
        assert_eq!(applied[0].change_type, ChangeType::Created);
        assert_eq!(
            std::fs::read_to_string(test_dir.path().join("file.txt")).unwrap(),
            "original"
        );
    }

    #[test]
    fn restore_twice_is_idempotent() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("file1.txt"), "changed").unwrap();

        let first = mgr.restore_to(&baseline).unwrap();
        assert!(!first.is_empty());

        let second = mgr.restore_to(&baseline).unwrap();
        assert!(second.is_empty());
    }

    #[test]
    fn corrupted_object_store_fails_restore() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        std::fs::write(test_dir.path().join("file1.txt"), "changed").unwrap();

        // Corrupt the object store by wiping the cache dir.
        let cache_dir = session_dir.path().join("session/cache/objects");
        if cache_dir.exists() {
            std::fs::remove_dir_all(&cache_dir).unwrap();
        }

        assert!(mgr.restore_to(&baseline).is_err());
    }

    #[test]
    fn incremental_no_changes_produces_empty_diff() {
        let test_dir = setup_test_dir();
        let session_dir = tempfile::tempdir().unwrap();
        let mut mgr = make_manager(session_dir.path(), test_dir.path());
        let baseline = mgr.create_baseline().unwrap();

        let (manifest, changes) = mgr.create_incremental(&baseline).unwrap();
        assert!(changes.is_empty());
        assert_eq!(baseline.merkle_root, manifest.merkle_root);
    }
}
