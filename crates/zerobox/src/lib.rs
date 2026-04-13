pub mod profile_core;
pub mod proxy;
mod sandbox;
pub mod secret;

pub use sandbox::Sandbox;
pub use sandbox::SandboxChild;
pub use sandbox::SandboxOutput;

pub fn zerobox_home() -> std::path::PathBuf {
    let path = std::env::var_os("ZEROBOX_HOME")
        .or_else(|| std::env::var_os("CODEX_HOME"))
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".zerobox")
        });
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(&path))
            .unwrap_or(path)
    }
}
