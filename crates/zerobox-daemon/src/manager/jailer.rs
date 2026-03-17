use std::path::PathBuf;

/// Configuration for the Firecracker jailer, which provides process isolation
/// by setting up a chroot environment, dropping privileges, and optionally
/// joining a network namespace.
pub struct JailerConfig {
    /// Unique identifier for this jailed instance.
    pub id: String,
    /// Path to the Firecracker binary.
    pub exec_file: PathBuf,
    /// UID to run the jailed process as.
    pub uid: u32,
    /// GID to run the jailed process as.
    pub gid: u32,
    /// Base directory for chroot environments.
    pub chroot_base_dir: PathBuf,
    /// Optional network namespace to join.
    pub netns: Option<String>,
}

impl JailerConfig {
    /// Builds the command-line arguments for invoking the jailer binary.
    ///
    /// These arguments follow the Firecracker jailer specification (section 10):
    /// --id, --exec-file, --uid, --gid, --chroot-base-dir, and optionally --netns.
    pub fn command_args(&self) -> Vec<String> {
        let mut args = vec![
            "--id".to_string(),
            self.id.clone(),
            "--exec-file".to_string(),
            self.exec_file.to_string_lossy().to_string(),
            "--uid".to_string(),
            self.uid.to_string(),
            "--gid".to_string(),
            self.gid.to_string(),
            "--chroot-base-dir".to_string(),
            self.chroot_base_dir.to_string_lossy().to_string(),
        ];

        if let Some(ref netns) = self.netns {
            args.push("--netns".to_string());
            args.push(netns.clone());
        }

        args
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_args_without_netns() {
        let config = JailerConfig {
            id: "sbx_abc123".to_string(),
            exec_file: PathBuf::from("/usr/local/bin/firecracker"),
            uid: 1000,
            gid: 1000,
            chroot_base_dir: PathBuf::from("/srv/jailer"),
            netns: None,
        };

        let args = config.command_args();
        assert_eq!(
            args,
            vec![
                "--id",
                "sbx_abc123",
                "--exec-file",
                "/usr/local/bin/firecracker",
                "--uid",
                "1000",
                "--gid",
                "1000",
                "--chroot-base-dir",
                "/srv/jailer",
            ]
        );
    }

    #[test]
    fn test_command_args_with_netns() {
        let config = JailerConfig {
            id: "sbx_abc123".to_string(),
            exec_file: PathBuf::from("/usr/local/bin/firecracker"),
            uid: 1000,
            gid: 1000,
            chroot_base_dir: PathBuf::from("/srv/jailer"),
            netns: Some("/var/run/netns/sbx_abc123".to_string()),
        };

        let args = config.command_args();
        assert!(args.contains(&"--netns".to_string()));
        assert!(args.contains(&"/var/run/netns/sbx_abc123".to_string()));
    }
}
