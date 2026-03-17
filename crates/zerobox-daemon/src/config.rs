use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub listen: String,
    pub data_dir: String,
    pub log_level: String,
    pub firecracker: FirecrackerConfig,
    pub networking: NetworkingConfig,
    pub timeouts: TimeoutsConfig,
    pub snapshots: SnapshotsConfig,
    pub images: ImagesConfig,
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FirecrackerConfig {
    pub binary: String,
    pub jailer: String,
    pub default_kernel: String,
    pub default_vcpus: u32,
    pub default_memory_mib: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NetworkingConfig {
    pub bridge: String,
    pub subnet: String,
    pub host_port_range: String,
    pub outbound_interface: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TimeoutsConfig {
    pub default_sandbox_timeout_ms: u64,
    pub max_sandbox_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SnapshotsConfig {
    pub storage_dir: String,
    pub max_snapshots_per_sandbox: u32,
    pub auto_snapshot_interval_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrebuiltImage {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ImagesConfig {
    pub cache_dir: String,
    #[serde(default)]
    pub prebuilt: Vec<PrebuiltImage>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub enabled: bool,
    #[serde(default)]
    pub tokens: Vec<String>,
}

pub fn load(path: &str) -> anyhow::Result<Config> {
    let contents = std::fs::read_to_string(path)
        .map_err(|e| anyhow::anyhow!("Failed to read config file '{}': {}", path, e))?;
    let config: Config = serde_yaml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("Failed to parse config file '{}': {}", path, e))?;
    Ok(config)
}
