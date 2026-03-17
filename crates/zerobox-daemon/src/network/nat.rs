use anyhow::{anyhow, Context, Result};
use tokio::process::Command;

/// Auto-detects the outbound network interface by parsing the default route.
pub async fn detect_outbound_interface() -> Result<String> {
    let output = Command::new("ip")
        .args(["route", "show", "default"])
        .output()
        .await
        .context("Failed to run 'ip route show default'")?;

    if !output.status.success() {
        return Err(anyhow!("'ip route show default' failed"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Parse "default via X.X.X.X dev <interface> ..."
    for part in stdout.split_whitespace().collect::<Vec<_>>().windows(2) {
        if part[0] == "dev" {
            return Ok(part[1].to_string());
        }
    }

    Err(anyhow!(
        "Could not find outbound interface in default route: {}",
        stdout.trim()
    ))
}

/// Sets up a NAT MASQUERADE rule for the sandbox subnet so guests can reach
/// the outside network via the host's outbound interface.
pub async fn setup_nat(outbound_interface: &str, subnet: &str) -> Result<()> {
    let status = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-A",
            "POSTROUTING",
            "-s",
            subnet,
            "-o",
            outbound_interface,
            "-j",
            "MASQUERADE",
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run iptables for NAT setup: {}", e))?;

    if !status.success() {
        return Err(anyhow!(
            "Failed to add NAT MASQUERADE rule for subnet {} on interface {}",
            subnet,
            outbound_interface
        ));
    }

    Ok(())
}

/// Removes the NAT MASQUERADE rule for the sandbox subnet.
pub async fn cleanup_nat(outbound_interface: &str, subnet: &str) -> Result<()> {
    let status = Command::new("iptables")
        .args([
            "-t",
            "nat",
            "-D",
            "POSTROUTING",
            "-s",
            subnet,
            "-o",
            outbound_interface,
            "-j",
            "MASQUERADE",
        ])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run iptables for NAT cleanup: {}", e))?;

    if !status.success() {
        return Err(anyhow!(
            "Failed to remove NAT MASQUERADE rule for subnet {} on interface {}",
            subnet,
            outbound_interface
        ));
    }

    Ok(())
}
