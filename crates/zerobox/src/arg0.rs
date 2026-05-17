//! Linux helper re-entry support for applications embedding zerobox.
//!
//! Call [`dispatch_linux_sandbox_helper`] near the start of `main`, then keep
//! the guard from [`prepend_path_entry_for_zerobox_aliases`] alive while running
//! sandboxed commands that use its helper path.
//!
//! ```no_run
//! # async fn run() -> Result<(), Box<dyn std::error::Error>> {
//! zerobox::arg0::dispatch_linux_sandbox_helper();
//! let guard = zerobox::arg0::prepend_path_entry_for_zerobox_aliases()?;
//!
//! let output = zerobox::Sandbox::command("echo")
//!     .arg("hello")
//!     .linux_sandbox_exe(guard.zerobox_linux_sandbox_exe())
//!     .run()
//!     .await?;
//! # drop(output);
//! # Ok(())
//! # }
//! ```

#[cfg(target_os = "linux")]
mod imp {
    use std::fs::File;
    use std::path::Path;
    use std::path::PathBuf;

    use tempfile::TempDir;
    use zerobox_sandboxing::landlock::ZEROBOX_LINUX_SANDBOX_ARG0;

    const LOCK_FILENAME: &str = ".lock";

    /// Keeps the temporary Linux helper alias alive for the current process.
    ///
    /// Hold this value for as long as sandboxed commands may need to re-enter
    /// the current binary as `zerobox-linux-sandbox`.
    pub struct Arg0PathEntryGuard {
        _temp_dir: TempDir,
        _lock_file: File,
        zerobox_linux_sandbox_exe: PathBuf,
    }

    impl Arg0PathEntryGuard {
        fn new(temp_dir: TempDir, lock_file: File, zerobox_linux_sandbox_exe: PathBuf) -> Self {
            Self {
                _temp_dir: temp_dir,
                _lock_file: lock_file,
                zerobox_linux_sandbox_exe,
            }
        }

        /// Path to the temporary `zerobox-linux-sandbox` alias.
        pub fn zerobox_linux_sandbox_exe(&self) -> &Path {
            &self.zerobox_linux_sandbox_exe
        }
    }

    /// If this process was invoked as `zerobox-linux-sandbox`, run the Linux
    /// sandbox helper and never return.
    ///
    /// Rust applications embedding zerobox and using Linux sandboxing should
    /// call this near the start of `main`, before argument parsing and before
    /// creating other threads.
    pub fn dispatch_linux_sandbox_helper() {
        if invoked_as_linux_sandbox_helper() {
            zerobox_linux_sandbox::run_main();
        }
    }

    /// Create a temporary PATH entry containing the helper aliases needed by
    /// Linux sandboxing.
    ///
    /// The returned guard must be kept alive while sandboxed commands may run.
    pub fn prepend_path_entry_for_zerobox_aliases() -> std::io::Result<Arg0PathEntryGuard> {
        let zerobox_home = crate::zerobox_home();
        ensure_arg0_home_allowed(&zerobox_home)?;
        std::fs::create_dir_all(&zerobox_home)?;

        let temp_root = zerobox_home.join("tmp").join("arg0");
        std::fs::create_dir_all(&temp_root)?;

        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp_root, std::fs::Permissions::from_mode(0o700))?;

        if let Err(err) = janitor_cleanup(&temp_root) {
            eprintln!("warning: failed to clean up stale arg0 helper dirs: {err}");
        }

        let temp_dir = tempfile::Builder::new()
            .prefix("zerobox-arg0")
            .tempdir_in(&temp_root)?;
        let path = temp_dir.path();

        let lock_path = path.join(LOCK_FILENAME);
        let lock_file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        lock_file.try_lock()?;

        let current_exe = std::env::current_exe()?;
        let zerobox_linux_sandbox_exe = path.join(ZEROBOX_LINUX_SANDBOX_ARG0);
        std::os::unix::fs::symlink(&current_exe, &zerobox_linux_sandbox_exe)?;

        let updated_path = path_with_entry_prepended(path, std::env::var_os("PATH"));
        // SAFETY: this is called by the CLI entrypoint before the Tokio runtime
        // is created, so no other application threads are running yet.
        unsafe {
            std::env::set_var("PATH", updated_path);
        }

        Ok(Arg0PathEntryGuard::new(
            temp_dir,
            lock_file,
            zerobox_linux_sandbox_exe,
        ))
    }

    fn invoked_as_linux_sandbox_helper() -> bool {
        let Some(argv0) = std::env::args_os().next() else {
            return false;
        };

        Path::new(&argv0)
            .file_name()
            .is_some_and(|name| name == std::ffi::OsStr::new(ZEROBOX_LINUX_SANDBOX_ARG0))
    }

    fn ensure_arg0_home_allowed(zerobox_home: &Path) -> std::io::Result<()> {
        #[cfg(not(debug_assertions))]
        {
            let temp_root = std::env::temp_dir();
            if arg0_home_is_under_temp_root(zerobox_home, &temp_root) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "refusing to create helper aliases under temporary dir {temp_root:?} \
                         (ZEROBOX_HOME: {zerobox_home:?})"
                    ),
                ));
            }
        }

        #[cfg(debug_assertions)]
        let _ = zerobox_home;

        Ok(())
    }

    #[cfg(any(test, not(debug_assertions)))]
    fn arg0_home_is_under_temp_root(zerobox_home: &Path, temp_root: &Path) -> bool {
        zerobox_home.starts_with(temp_root)
    }

    fn janitor_cleanup(temp_root: &Path) -> std::io::Result<()> {
        let entries = match std::fs::read_dir(temp_root) {
            Ok(entries) => entries,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(err) => return Err(err),
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(_lock_file) = try_lock_dir(&path)? else {
                continue;
            };

            match std::fs::remove_dir_all(&path) {
                Ok(()) => {}
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
                Err(err) => return Err(err),
            }
        }

        Ok(())
    }

    fn try_lock_dir(dir: &Path) -> std::io::Result<Option<File>> {
        let lock_path = dir.join(LOCK_FILENAME);
        let lock_file = match File::options().read(true).write(true).open(&lock_path) {
            Ok(file) => file,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };

        match lock_file.try_lock() {
            Ok(()) => Ok(Some(lock_file)),
            Err(std::fs::TryLockError::WouldBlock) => Ok(None),
            Err(err) => Err(err.into()),
        }
    }

    fn path_with_entry_prepended(
        entry: &Path,
        existing_path: Option<std::ffi::OsString>,
    ) -> std::ffi::OsString {
        const PATH_SEPARATOR: &str = ":";

        match existing_path {
            Some(existing_path) => {
                let mut path_env_var = std::ffi::OsString::with_capacity(
                    entry.as_os_str().len() + PATH_SEPARATOR.len() + existing_path.len(),
                );
                path_env_var.push(entry);
                path_env_var.push(PATH_SEPARATOR);
                path_env_var.push(existing_path);
                path_env_var
            }
            None => entry.as_os_str().to_owned(),
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn create_lock(dir: &Path) -> std::io::Result<File> {
            File::options()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(dir.join(LOCK_FILENAME))
        }

        #[test]
        fn path_with_entry_prepended_adds_separator_when_path_exists() {
            let result = path_with_entry_prepended(
                Path::new("/tmp/alias"),
                Some(std::ffi::OsString::from("/usr/bin")),
            );

            assert_eq!(result, std::ffi::OsString::from("/tmp/alias:/usr/bin"));
        }

        #[test]
        fn path_with_entry_prepended_uses_entry_when_path_is_missing() {
            let result = path_with_entry_prepended(Path::new("/tmp/alias"), None);

            assert_eq!(result, std::ffi::OsString::from("/tmp/alias"));
        }

        #[test]
        fn arg0_home_is_under_temp_root_matches_nested_paths() -> std::io::Result<()> {
            let root = tempfile::tempdir()?;
            let home = root.path().join("zerobox-home");

            assert!(arg0_home_is_under_temp_root(&home, root.path()));
            Ok(())
        }

        #[test]
        fn arg0_home_is_under_temp_root_rejects_sibling_prefixes() {
            assert!(!arg0_home_is_under_temp_root(
                Path::new("/tmp/zerobox-other"),
                Path::new("/tmp/zerobox"),
            ));
        }

        #[test]
        fn janitor_cleanup_skips_dirs_without_lock_file() -> std::io::Result<()> {
            let root = tempfile::tempdir()?;
            let dir = root.path().join("no-lock");
            std::fs::create_dir(&dir)?;

            janitor_cleanup(root.path())?;

            assert!(dir.exists());
            Ok(())
        }

        #[test]
        fn janitor_cleanup_skips_dirs_with_held_lock() -> std::io::Result<()> {
            let root = tempfile::tempdir()?;
            let dir = root.path().join("locked");
            std::fs::create_dir(&dir)?;
            let lock_file = create_lock(&dir)?;
            lock_file.try_lock()?;

            janitor_cleanup(root.path())?;

            assert!(dir.exists());
            Ok(())
        }

        #[test]
        fn janitor_cleanup_removes_unlocked_stale_dirs() -> std::io::Result<()> {
            let root = tempfile::tempdir()?;
            let stale = root.path().join("zerobox-arg0-stale");
            std::fs::create_dir(&stale)?;
            create_lock(&stale)?;

            janitor_cleanup(root.path())?;

            assert!(!stale.exists());
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
pub use imp::Arg0PathEntryGuard;

#[cfg(target_os = "linux")]
pub use imp::dispatch_linux_sandbox_helper;

#[cfg(target_os = "linux")]
pub use imp::prepend_path_entry_for_zerobox_aliases;
