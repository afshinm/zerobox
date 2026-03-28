/// Thin shim for codex-core. Only provides the error types that
/// `codex-linux-sandbox` actually imports.
pub mod error {
    use std::io;
    use thiserror::Error;

    pub type Result<T> = std::result::Result<T, CodexErr>;

    #[derive(Error, Debug)]
    pub enum SandboxErr {
        #[cfg(target_os = "linux")]
        #[error("seccomp setup error")]
        SeccompInstall(#[from] seccompiler::Error),

        #[cfg(target_os = "linux")]
        #[error("seccomp backend error")]
        SeccompBackend(#[from] seccompiler::BackendError),

        #[error("command was killed by a signal")]
        Signal(i32),

        #[error("Landlock was not able to fully enforce all sandbox rules")]
        LandlockRestrict,
    }

    #[derive(Error, Debug)]
    pub enum CodexErr {
        #[error("sandbox error: {0}")]
        Sandbox(#[from] SandboxErr),

        #[error("unsupported operation: {0}")]
        UnsupportedOperation(String),

        #[error(transparent)]
        Io(#[from] io::Error),

        #[cfg(target_os = "linux")]
        #[error(transparent)]
        LandlockRuleset(#[from] landlock::RulesetError),

        #[cfg(target_os = "linux")]
        #[error(transparent)]
        LandlockPathFd(#[from] landlock::PathFdError),
    }
}
