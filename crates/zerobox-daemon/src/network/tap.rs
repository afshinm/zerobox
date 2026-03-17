use anyhow::{anyhow, Result};
use tokio::process::Command;

/// Creates a tap device, adds it to the specified bridge, and brings it up.
pub async fn create_tap(name: &str, bridge: &str) -> Result<()> {
    // Create the tap device
    let status = Command::new("ip")
        .args(["tuntap", "add", "dev", name, "mode", "tap"])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to create tap device '{}': {}", name, e))?;

    if !status.success() {
        return Err(anyhow!("Failed to create tap device '{}'", name));
    }

    // Add tap to bridge
    let status = Command::new("ip")
        .args(["link", "set", name, "master", bridge])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to add tap '{}' to bridge '{}': {}", name, bridge, e))?;

    if !status.success() {
        return Err(anyhow!(
            "Failed to add tap '{}' to bridge '{}'",
            name,
            bridge
        ));
    }

    // Bring up the tap device
    let status = Command::new("ip")
        .args(["link", "set", name, "up"])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to bring up tap device '{}': {}", name, e))?;

    if !status.success() {
        return Err(anyhow!("Failed to bring up tap device '{}'", name));
    }

    Ok(())
}

/// Deletes a tap device.
pub async fn delete_tap(name: &str) -> Result<()> {
    let status = Command::new("ip")
        .args(["link", "delete", name])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to delete tap device '{}': {}", name, e))?;

    if !status.success() {
        return Err(anyhow!("Failed to delete tap device '{}'", name));
    }

    Ok(())
}

/// Generates a tap device name from a sandbox ID.
/// Uses the first 8 characters of the sandbox ID (after the "sbx_" prefix) to keep
/// the name within Linux's 15-character interface name limit.
pub fn tap_name(sandbox_id: &str) -> String {
    let short = sandbox_id.strip_prefix("sbx_").unwrap_or(sandbox_id);
    let truncated = &short[..short.len().min(8)];
    format!("tap-{}", truncated)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tap_name() {
        assert_eq!(tap_name("sbx_abc123def456"), "tap-abc123de");
        assert_eq!(tap_name("sbx_abcd"), "tap-abcd");
    }
}
