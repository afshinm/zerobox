use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{anyhow, Result};
use tokio::process::Command;

/// Adds a DNAT port forwarding rule so that traffic arriving on the host at
/// `host_port` is forwarded to `guest_ip:guest_port`.
pub async fn add_port_forward(host_port: u16, guest_ip: &str, guest_port: u16) -> Result<()> {
    let dnat_target = format!("{}:{}", guest_ip, guest_port);

    let status = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-A",
            "PREROUTING",
            "-p",
            "tcp",
            "--dport",
            &host_port.to_string(),
            "-j",
            "DNAT",
            "--to-destination",
            &dnat_target,
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run iptables for port forward: {}", e))?;

    if !status.success() {
        return Err(anyhow!(
            "Failed to add port forward {}->{}:{}",
            host_port,
            guest_ip,
            guest_port
        ));
    }

    Ok(())
}

/// Removes a DNAT port forwarding rule.
pub async fn remove_port_forward(host_port: u16, guest_ip: &str, guest_port: u16) -> Result<()> {
    let dnat_target = format!("{}:{}", guest_ip, guest_port);

    let status = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-D",
            "PREROUTING",
            "-p",
            "tcp",
            "--dport",
            &host_port.to_string(),
            "-j",
            "DNAT",
            "--to-destination",
            &dnat_target,
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run iptables for port forward removal: {}", e))?;

    if !status.success() {
        return Err(anyhow!(
            "Failed to remove port forward {}->{}:{}",
            host_port,
            guest_ip,
            guest_port
        ));
    }

    Ok(())
}

/// Parses a port range string (e.g., "30000-40000") and returns the next
/// available port using an atomic counter to track allocation state.
/// Uses AtomicU32 to avoid wrapping issues with u16 (which would silently
/// re-issue already-allocated ports after 65536 calls).
pub fn allocate_host_port(range: &str, counter: &AtomicU32) -> Result<u16> {
    let parts: Vec<&str> = range.split('-').collect();
    if parts.len() != 2 {
        return Err(anyhow!(
            "Invalid port range format: '{}'. Expected 'start-end'.",
            range
        ));
    }

    let start: u32 = parts[0]
        .parse()
        .map_err(|_| anyhow!("Invalid start port in range: '{}'", parts[0]))?;
    let end: u32 = parts[1]
        .parse()
        .map_err(|_| anyhow!("Invalid end port in range: '{}'", parts[1]))?;

    if start > end {
        return Err(anyhow!(
            "Invalid port range: start ({}) > end ({})",
            start,
            end
        ));
    }

    let offset = counter.fetch_add(1, Ordering::Relaxed);
    let port = start + offset;
    if port > end {
        return Err(anyhow!(
            "No more ports available in range {}-{}",
            start,
            end
        ));
    }
    Ok(port as u16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_host_port() {
        let counter = AtomicU32::new(0);
        let port = allocate_host_port("30000-40000", &counter).unwrap();
        assert_eq!(port, 30000);
        let port2 = allocate_host_port("30000-40000", &counter).unwrap();
        assert_eq!(port2, 30001);
    }

    #[test]
    fn test_allocate_host_port_invalid() {
        let counter = AtomicU32::new(0);
        assert!(allocate_host_port("invalid", &counter).is_err());
        assert!(allocate_host_port("40000-30000", &counter).is_err());
    }

    #[test]
    fn test_allocate_host_port_exhaustion() {
        let counter = AtomicU32::new(0);
        let port = allocate_host_port("30000-30001", &counter).unwrap();
        assert_eq!(port, 30000);
        let port2 = allocate_host_port("30000-30001", &counter).unwrap();
        assert_eq!(port2, 30001);
        assert!(allocate_host_port("30000-30001", &counter).is_err());
    }
}
