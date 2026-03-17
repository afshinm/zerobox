pub mod bridge;
pub mod nat;
pub mod port_forward;
pub mod tap;

use std::sync::atomic::AtomicU32;

use anyhow::Result;

use crate::config::NetworkingConfig;

pub struct NetworkManager {
    config: NetworkingConfig,
    port_counter: AtomicU32,
}

impl NetworkManager {
    pub fn new(config: NetworkingConfig) -> Self {
        Self {
            config,
            port_counter: AtomicU32::new(0),
        }
    }

    /// Sets up the Linux bridge for sandbox networking.
    pub async fn setup_bridge(&self) -> Result<()> {
        bridge::create_bridge(&self.config.bridge).await
    }

    /// Sets up networking for a sandbox: creates a tap device, attaches it to the
    /// bridge, and configures port forwarding for the requested ports.
    ///
    /// Returns a list of (host_port, guest_port) mappings.
    pub async fn setup_sandbox_networking(
        &self,
        sandbox_id: &str,
        guest_ip: &str,
        ports: &[u16],
    ) -> Result<Vec<(u16, u16)>> {
        let tap_name = tap::tap_name(sandbox_id);
        tap::create_tap(&tap_name, &self.config.bridge).await?;

        let mut mappings = Vec::new();
        for &guest_port in ports {
            let host_port =
                port_forward::allocate_host_port(&self.config.host_port_range, &self.port_counter)?;
            port_forward::add_port_forward(host_port, guest_ip, guest_port).await?;
            mappings.push((host_port, guest_port));
        }

        Ok(mappings)
    }

    /// Cleans up all networking resources for a sandbox: removes port forwarding rules,
    /// deletes the tap device.
    pub async fn cleanup_sandbox_networking(
        &self,
        sandbox_id: &str,
        guest_ip: &str,
        port_mappings: &[(u16, u16)],
    ) -> Result<()> {
        // Remove port forwarding rules first
        for &(host_port, guest_port) in port_mappings {
            if let Err(e) = port_forward::remove_port_forward(host_port, guest_ip, guest_port).await
            {
                tracing::warn!(
                    sandbox_id,
                    host_port,
                    guest_port,
                    "Failed to remove port forward rule: {}",
                    e
                );
            }
        }

        // Delete tap device
        let tap_name = tap::tap_name(sandbox_id);
        tap::delete_tap(&tap_name).await?;
        Ok(())
    }
}
