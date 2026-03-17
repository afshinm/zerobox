pub mod firecracker;
pub mod jailer;
pub mod lifecycle;
pub mod process;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use thiserror::Error;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::guest::GuestClient;
use crate::network;
use zerobox_common::protocol::*;
use zerobox_common::types::*;

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("Sandbox not found: {0}")]
    NotFound(String),
    #[error("Sandbox not in expected state: {0}")]
    InvalidState(String),
    #[error("{0}")]
    Internal(#[from] anyhow::Error),
}

pub struct SandboxState {
    pub info: SandboxInfo,
    pub process: Option<process::FirecrackerProcess>,
    pub socket_path: PathBuf,
    pub rootfs_path: PathBuf,
    pub log_path: PathBuf,
    pub vsock_path: PathBuf,
    pub guest_ip: String,
    pub port_mappings: Vec<(u16, u16)>,
    pub commands: HashMap<String, CommandState>,
}

pub struct CommandState {
    pub info: CommandInfo,
    pub stdout: String,
    pub stderr: String,
    pub running: bool,
}

pub struct IpPool {
    next_counter: u32,
    free_list: Vec<u32>,
}

impl IpPool {
    fn new() -> Self {
        Self {
            // Start at 2: skip .0 (network) and .1 (bridge/host gateway)
            next_counter: 2,
            free_list: Vec::new(),
        }
    }

    fn allocate(&mut self) -> Result<String, SandboxError> {
        // Prefer recycled IPs from the free list
        let counter = if let Some(recycled) = self.free_list.pop() {
            recycled
        } else {
            if self.next_counter > 65534 {
                return Err(SandboxError::Internal(anyhow::anyhow!(
                    "IP address pool exhausted (10.20.0.0/16 supports max ~65k sandboxes)"
                )));
            }
            let c = self.next_counter;
            self.next_counter += 1;
            c
        };
        Ok(lifecycle::allocate_ip(counter))
    }

    fn release(&mut self, ip: &str) {
        // Parse the IP back to a counter value
        if let Some(counter) = lifecycle::ip_to_counter(ip) {
            self.free_list.push(counter);
        }
    }
}

pub struct SandboxManager {
    sandboxes: RwLock<HashMap<String, SandboxState>>,
    config: Arc<Config>,
    ip_pool: RwLock<IpPool>,
    network: network::NetworkManager,
}

impl SandboxManager {
    pub fn new(config: Arc<Config>, network: network::NetworkManager) -> Self {
        Self {
            sandboxes: RwLock::new(HashMap::new()),
            config,
            ip_pool: RwLock::new(IpPool::new()),
            network,
        }
    }

    /// Boots a Firecracker VM: spawns the process, configures it via the REST
    /// API, starts the instance, and waits for the guest agent to become
    /// healthy. Returns the `FirecrackerProcess` on success.
    async fn boot_vm(
        &self,
        sandbox_id: &str,
        sandbox_dir: &Path,
        rootfs_path: &Path,
        vsock_path: &Path,
        vcpus: u32,
        memory_mib: u32,
    ) -> Result<process::FirecrackerProcess, anyhow::Error> {
        // --- Step 1: Spawn the Firecracker process ---
        let mut fc_process = process::FirecrackerProcess::spawn(
            &self.config.firecracker.binary,
            sandbox_id,
            sandbox_dir,
        )
        .await?;

        // Helper: on any subsequent failure, kill the process before returning.
        let socket_path = fc_process.socket_path.clone();
        let vsock_uds = fc_process.vsock_path.clone();

        // --- Step 2: Wait for the API socket to appear ---
        if !process::wait_for_socket(&socket_path, Duration::from_secs(5)).await {
            let _ = fc_process.kill().await;
            return Err(anyhow::anyhow!(
                "Firecracker API socket did not appear within 5 seconds"
            ));
        }

        // --- Step 3: Configure VM via Firecracker REST API ---
        let fc = firecracker::FirecrackerClient::new(&socket_path);

        // Determine kernel path based on architecture
        let arch = match std::env::consts::ARCH {
            "aarch64" => "aarch64",
            _ => "x86_64",
        };
        let kernel_path = format!(
            "{}/kernels/{}-{}",
            self.config.data_dir, self.config.firecracker.default_kernel, arch
        );

        if let Err(e) = fc
            .set_boot_source(&kernel_path, "console=ttyS0 reboot=k panic=1 pci=off")
            .await
        {
            let _ = fc_process.kill().await;
            return Err(anyhow::anyhow!("Failed to set boot source: {}", e));
        }

        let rootfs_str = rootfs_path.to_string_lossy();
        if let Err(e) = fc.set_drives("rootfs", &rootfs_str, true, false).await {
            let _ = fc_process.kill().await;
            return Err(anyhow::anyhow!("Failed to set drives: {}", e));
        }

        if let Err(e) = fc.set_machine_config(vcpus, memory_mib).await {
            let _ = fc_process.kill().await;
            return Err(anyhow::anyhow!("Failed to set machine config: {}", e));
        }

        // Network interface (tap device)
        let tap_name = network::tap::tap_name(sandbox_id);
        if let Err(e) = fc.set_network_interface("eth0", &tap_name).await {
            let _ = fc_process.kill().await;
            return Err(anyhow::anyhow!("Failed to set network interface: {}", e));
        }

        // Vsock
        let vsock_str = vsock_uds.to_string_lossy();
        if let Err(e) = fc.set_vsock("vsock0", 3, &vsock_str).await {
            let _ = fc_process.kill().await;
            return Err(anyhow::anyhow!("Failed to set vsock: {}", e));
        }

        // --- Step 4: Start the instance ---
        if let Err(e) = fc.start_instance().await {
            let _ = fc_process.kill().await;
            return Err(anyhow::anyhow!("Failed to start instance: {}", e));
        }

        // --- Step 5: Wait for guest agent health ---
        // Poll up to 15 seconds (30 retries x 500ms). Failure is not fatal —
        // the VM booted but the agent may not be ready yet.
        let guest = GuestClient::new(vsock_path);
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(500)).await;
            if guest.health().await.is_ok() {
                tracing::info!(sandbox_id = %sandbox_id, "Guest agent is healthy");
                return Ok(fc_process);
            }
        }

        tracing::warn!(
            sandbox_id = %sandbox_id,
            "Guest agent did not become healthy within 15 seconds; VM is running but agent may not be ready"
        );
        Ok(fc_process)
    }

    pub async fn create(&self, req: CreateSandboxRequest) -> Result<SandboxInfo, SandboxError> {
        let sandbox_id = lifecycle::generate_sandbox_id();
        let sandbox_dir = lifecycle::sandbox_dir(&self.config.data_dir, &sandbox_id);

        // Allocate an IP address from the 10.20.0.0/16 pool
        let guest_ip = {
            let mut pool = self.ip_pool.write().await;
            pool.allocate()?
        };

        // Determine resources
        let vcpus = req
            .resources
            .as_ref()
            .and_then(|r| r.vcpus)
            .unwrap_or(self.config.firecracker.default_vcpus);
        let memory_mib = req
            .resources
            .as_ref()
            .and_then(|r| r.memory_mib)
            .unwrap_or(self.config.firecracker.default_memory_mib);

        let timeout = req
            .timeout
            .unwrap_or(self.config.timeouts.default_sandbox_timeout_ms);

        let ports = req.ports.clone().unwrap_or_default();

        // Create sandbox directory
        tokio::fs::create_dir_all(&sandbox_dir)
            .await
            .map_err(|e| SandboxError::Internal(e.into()))?;

        // Construct paths
        let socket_path = sandbox_dir.join("firecracker.sock");
        let vsock_path = sandbox_dir.join("vsock.sock");
        let log_path = sandbox_dir.join("firecracker.log");

        // Determine rootfs source image name
        let image_name = if let Some(ref source) = req.source {
            match source.source_type {
                SourceType::Image => source.image.as_deref().unwrap_or("default"),
                _ => "default",
            }
        } else {
            "default"
        };

        // Prepare rootfs — copy from pre-built image cache into sandbox dir
        let rootfs_path = sandbox_dir.join("rootfs.ext4");
        let cached_image =
            PathBuf::from(&self.config.images.cache_dir).join(format!("{}.ext4", image_name));
        if cached_image.exists() {
            tokio::fs::copy(&cached_image, &rootfs_path)
                .await
                .map_err(|e| {
                    SandboxError::Internal(anyhow::anyhow!(
                        "Failed to copy rootfs from {:?} to {:?}: {}",
                        cached_image,
                        rootfs_path,
                        e
                    ))
                })?;
        } else {
            tracing::warn!(
                sandbox_id = %sandbox_id,
                "Pre-built image {:?} not found; sandbox will have no rootfs",
                cached_image
            );
        }

        // Set up networking — on failure (e.g. macOS), log a warning but continue
        let port_mappings = match self
            .network
            .setup_sandbox_networking(&sandbox_id, &guest_ip, &ports)
            .await
        {
            Ok(mappings) => mappings,
            Err(e) => {
                tracing::warn!(
                    sandbox_id = %sandbox_id,
                    "Failed to setup sandbox networking: {}. Continuing without network.",
                    e
                );
                Vec::new()
            }
        };

        // Build port map for SandboxInfo from actual mappings
        let mut port_map = HashMap::new();
        for &(host_port, guest_port) in &port_mappings {
            port_map.insert(guest_port.to_string(), host_port.to_string());
        }
        // If no actual mappings but ports were requested, map them 1:1 as fallback
        if port_mappings.is_empty() {
            for p in &ports {
                port_map.insert(p.to_string(), p.to_string());
            }
        }

        let now = chrono::Utc::now().to_rfc3339();

        // Try to boot the VM. If Firecracker is not available (e.g. macOS),
        // register the sandbox as Failed rather than returning an error.
        let (fc_process, status) = match self
            .boot_vm(
                &sandbox_id,
                &sandbox_dir,
                &rootfs_path,
                &vsock_path,
                vcpus,
                memory_mib,
            )
            .await
        {
            Ok(proc) => (Some(proc), SandboxStatus::Running),
            Err(e) => {
                tracing::warn!(
                    sandbox_id = %sandbox_id,
                    "Failed to boot VM: {}. Sandbox registered as failed.",
                    e
                );
                (None, SandboxStatus::Failed)
            }
        };

        let info = SandboxInfo {
            sandbox_id: sandbox_id.clone(),
            status,
            ip: Some(guest_ip.clone()),
            created_at: now,
            timeout,
            ports: port_map,
        };

        let state = SandboxState {
            info: info.clone(),
            process: fc_process,
            socket_path,
            rootfs_path,
            log_path,
            vsock_path,
            guest_ip,
            port_mappings,
            commands: HashMap::new(),
        };

        let mut sandboxes = self.sandboxes.write().await;
        sandboxes.insert(sandbox_id, state);

        Ok(info)
    }

    pub async fn list(&self) -> Result<Vec<SandboxInfo>, SandboxError> {
        let sandboxes = self.sandboxes.read().await;
        let list: Vec<SandboxInfo> = sandboxes.values().map(|s| s.info.clone()).collect();
        Ok(list)
    }

    pub async fn get(&self, id: &str) -> Result<SandboxInfo, SandboxError> {
        let sandboxes = self.sandboxes.read().await;
        let state = sandboxes
            .get(id)
            .ok_or_else(|| SandboxError::NotFound(id.to_string()))?;
        Ok(state.info.clone())
    }

    pub async fn stop(&self, id: &str) -> Result<SandboxInfo, SandboxError> {
        let mut sandboxes = self.sandboxes.write().await;
        let state = sandboxes
            .get_mut(id)
            .ok_or_else(|| SandboxError::NotFound(id.to_string()))?;

        // Try to gracefully stop via Firecracker API
        if state.process.is_some() {
            let fc = firecracker::FirecrackerClient::new(&state.socket_path);
            if let Err(e) = fc.stop_instance().await {
                tracing::warn!(sandbox_id = %id, "Graceful stop failed: {}, killing process", e);
            }
            // Kill the process
            if let Some(ref mut proc) = state.process {
                let _ = proc.kill().await;
            }
            state.process = None;
        }

        state.info.status = SandboxStatus::Stopped;
        Ok(state.info.clone())
    }

    pub async fn destroy(&self, id: &str) -> Result<(), SandboxError> {
        let (sandbox_dir, guest_ip, port_mappings, mut fc_process) = {
            let mut sandboxes = self.sandboxes.write().await;
            let state = sandboxes
                .remove(id)
                .ok_or_else(|| SandboxError::NotFound(id.to_string()))?;
            let dir = lifecycle::sandbox_dir(&self.config.data_dir, id);
            (
                dir,
                state.guest_ip.clone(),
                state.port_mappings.clone(),
                state.process,
            )
        };

        // Kill Firecracker process if still running
        if let Some(ref mut proc) = fc_process {
            let _ = proc.kill().await;
        }

        // Clean up networking
        if let Err(e) = self
            .network
            .cleanup_sandbox_networking(id, &guest_ip, &port_mappings)
            .await
        {
            tracing::warn!(sandbox_id = %id, "Network cleanup failed: {}", e);
        }

        // Release IP back to the pool
        {
            let mut pool = self.ip_pool.write().await;
            pool.release(&guest_ip);
        }

        // Remove sandbox directory (best-effort)
        let _ = tokio::fs::remove_dir_all(&sandbox_dir).await;

        Ok(())
    }

    pub async fn run_command(
        &self,
        sandbox_id: &str,
        req: RunCommandRequest,
    ) -> Result<CommandInfo, SandboxError> {
        let vsock_path = {
            let sandboxes = self.sandboxes.read().await;
            let state = sandboxes
                .get(sandbox_id)
                .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
            state.vsock_path.clone()
        }; // lock dropped here

        let guest = GuestClient::new(&vsock_path);
        let is_detached = req.detached.unwrap_or(false);
        let cwd = req.cwd.clone().unwrap_or_else(|| "/".to_string());
        let params = ExecParams {
            cmd: req.cmd,
            args: req.args.unwrap_or_default(),
            cwd: req.cwd,
            env: req.env,
            sudo: req.sudo.unwrap_or(false),
        };

        let now = chrono::Utc::now().timestamp_millis() as u64;

        let info = if is_detached {
            let result = guest.exec_detached(params).await?;
            CommandInfo {
                cmd_id: result.cmd_id,
                exit_code: None,
                started_at: now,
                cwd: cwd.clone(),
                stdout: None,
                stderr: None,
            }
        } else {
            let result = guest.exec(params).await?;
            CommandInfo {
                cmd_id: result.cmd_id,
                exit_code: result.exit_code,
                started_at: now,
                cwd: cwd.clone(),
                stdout: Some(result.stdout),
                stderr: Some(result.stderr),
            }
        };

        // Track command state in the daemon so get_command can return real values
        {
            let mut sandboxes = self.sandboxes.write().await;
            if let Some(state) = sandboxes.get_mut(sandbox_id) {
                state.commands.insert(
                    info.cmd_id.clone(),
                    CommandState {
                        info: info.clone(),
                        stdout: String::new(),
                        stderr: String::new(),
                        running: info.exit_code.is_none(),
                    },
                );
            }
        }

        Ok(info)
    }

    pub async fn get_command(
        &self,
        sandbox_id: &str,
        cmd_id: &str,
    ) -> Result<CommandInfo, SandboxError> {
        // First, check if we have the command tracked in daemon state
        let (vsock_path, tracked_info) = {
            let sandboxes = self.sandboxes.read().await;
            let state = sandboxes
                .get(sandbox_id)
                .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
            let tracked = state.commands.get(cmd_id).map(|cs| cs.info.clone());
            (state.vsock_path.clone(), tracked)
        }; // lock dropped here

        // Query guest agent for live status
        let guest = GuestClient::new(&vsock_path);
        let result = guest.get_command(cmd_id).await?;

        // Use tracked started_at/cwd if available, otherwise fall back to defaults
        let (started_at, cwd) = match tracked_info {
            Some(ref info) => (info.started_at, info.cwd.clone()),
            None => (
                chrono::Utc::now().timestamp_millis() as u64,
                "/".to_string(),
            ),
        };

        let info = CommandInfo {
            cmd_id: result.cmd_id,
            exit_code: result.exit_code,
            started_at,
            cwd,
            stdout: None,
            stderr: None,
        };

        // Update tracked state with latest exit_code from guest
        if result.exit_code.is_some() {
            let mut sandboxes = self.sandboxes.write().await;
            if let Some(state) = sandboxes.get_mut(sandbox_id) {
                if let Some(cs) = state.commands.get_mut(cmd_id) {
                    cs.info.exit_code = result.exit_code;
                    cs.running = false;
                }
            }
        }

        Ok(info)
    }

    pub async fn kill_command(&self, sandbox_id: &str, cmd_id: &str) -> Result<(), SandboxError> {
        let vsock_path = {
            let sandboxes = self.sandboxes.read().await;
            let state = sandboxes
                .get(sandbox_id)
                .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
            state.vsock_path.clone()
        }; // lock dropped here

        let guest = GuestClient::new(&vsock_path);
        guest
            .kill(KillParams {
                cmd_id: cmd_id.to_string(),
                signal: None,
            })
            .await?;
        Ok(())
    }

    pub async fn write_files(
        &self,
        sandbox_id: &str,
        req: WriteFilesRequest,
    ) -> Result<(), SandboxError> {
        let vsock_path = {
            let sandboxes = self.sandboxes.read().await;
            let state = sandboxes
                .get(sandbox_id)
                .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
            state.vsock_path.clone()
        }; // lock dropped here

        let guest = GuestClient::new(&vsock_path);
        for file in &req.files {
            guest
                .write_file(WriteFileParams {
                    path: file.path.clone(),
                    content: file.content.clone(),
                })
                .await?;
        }
        Ok(())
    }

    pub async fn read_file(
        &self,
        sandbox_id: &str,
        path: &str,
    ) -> Result<Option<Vec<u8>>, SandboxError> {
        let vsock_path = {
            let sandboxes = self.sandboxes.read().await;
            let state = sandboxes
                .get(sandbox_id)
                .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
            state.vsock_path.clone()
        }; // lock dropped here

        let guest = GuestClient::new(&vsock_path);
        let result = guest.read_file(path).await?;

        if !result.exists {
            return Ok(None);
        }

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&result.content)
            .map_err(|e| SandboxError::Internal(e.into()))?;
        Ok(Some(bytes))
    }

    pub async fn mkdir(&self, sandbox_id: &str, path: &str) -> Result<(), SandboxError> {
        let vsock_path = {
            let sandboxes = self.sandboxes.read().await;
            let state = sandboxes
                .get(sandbox_id)
                .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
            state.vsock_path.clone()
        }; // lock dropped here

        let guest = GuestClient::new(&vsock_path);
        guest.mkdir(path).await?;
        Ok(())
    }

    pub async fn extend_timeout(
        &self,
        sandbox_id: &str,
        duration_ms: u64,
    ) -> Result<(), SandboxError> {
        let mut sandboxes = self.sandboxes.write().await;
        let state = sandboxes
            .get_mut(sandbox_id)
            .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;

        let new_timeout = state.info.timeout + duration_ms;
        let max = self.config.timeouts.max_sandbox_timeout_ms;
        if new_timeout > max {
            return Err(SandboxError::InvalidState(format!(
                "Timeout would exceed maximum ({} ms > {} ms)",
                new_timeout, max
            )));
        }
        state.info.timeout = new_timeout;
        Ok(())
    }

    pub async fn create_snapshot(
        &self,
        sandbox_id: &str,
        snapshot_manager: &crate::snapshot::SnapshotManager,
    ) -> Result<zerobox_common::types::SnapshotInfo, SandboxError> {
        // Verify sandbox exists and is running, and grab the socket path
        let socket_path = {
            let sandboxes = self.sandboxes.read().await;
            let state = sandboxes
                .get(sandbox_id)
                .ok_or_else(|| SandboxError::NotFound(sandbox_id.to_string()))?;
            if state.info.status != SandboxStatus::Running
                && state.info.status != SandboxStatus::Pending
            {
                return Err(SandboxError::InvalidState(format!(
                    "Sandbox {} is not running (status: {:?})",
                    sandbox_id, state.info.status
                )));
            }
            state.socket_path.clone()
        };

        // Pause the VM (best-effort — snapshot can still proceed)
        let fc = firecracker::FirecrackerClient::new(&socket_path);
        if let Err(e) = fc.pause_instance().await {
            tracing::warn!(
                sandbox_id = %sandbox_id,
                "Failed to pause VM for snapshot: {}",
                e
            );
        }

        // Create snapshot metadata
        let snapshot_info = snapshot_manager
            .create(sandbox_id)
            .await
            .map_err(|e| SandboxError::Internal(anyhow::anyhow!("{}", e)))?;

        // Stop the sandbox (spec: snapshot stops the sandbox).
        // Kill the Firecracker process since the VM is already paused.
        {
            let mut sandboxes = self.sandboxes.write().await;
            if let Some(state) = sandboxes.get_mut(sandbox_id) {
                if let Some(ref mut proc) = state.process {
                    let _ = proc.kill().await;
                }
                state.process = None;
                state.info.status = SandboxStatus::Stopped;
            }
        }

        Ok(snapshot_info)
    }
}
