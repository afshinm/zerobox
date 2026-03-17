use anyhow::{anyhow, Result};
use tokio::process::Command;

/// Creates a Linux bridge with the given name and brings it up.
/// If the bridge already exists, this is a no-op.
pub async fn create_bridge(name: &str) -> Result<()> {
    if bridge_exists(name).await? {
        return Ok(());
    }

    // Create the bridge device
    let status = Command::new("ip")
        .args(["link", "add", "name", name, "type", "bridge"])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run 'ip link add': {}", e))?;

    if !status.success() {
        return Err(anyhow!("Failed to create bridge '{}'", name));
    }

    // Bring the bridge up
    let status = Command::new("ip")
        .args(["link", "set", name, "up"])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run 'ip link set up': {}", e))?;

    if !status.success() {
        return Err(anyhow!("Failed to bring up bridge '{}'", name));
    }

    Ok(())
}

/// Deletes a Linux bridge.
pub async fn delete_bridge(name: &str) -> Result<()> {
    let status = Command::new("ip")
        .args(["link", "set", name, "down"])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run 'ip link set down': {}", e))?;

    if !status.success() {
        return Err(anyhow!("Failed to bring down bridge '{}'", name));
    }

    let status = Command::new("ip")
        .args(["link", "delete", name, "type", "bridge"])
        .status()
        .await
        .map_err(|e| anyhow!("Failed to run 'ip link delete': {}", e))?;

    if !status.success() {
        return Err(anyhow!("Failed to delete bridge '{}'", name));
    }

    Ok(())
}

/// Checks whether a bridge with the given name exists.
pub async fn bridge_exists(name: &str) -> Result<bool> {
    let output = Command::new("ip")
        .args(["link", "show", name])
        .output()
        .await
        .map_err(|e| anyhow!("Failed to run 'ip link show': {}", e))?;

    Ok(output.status.success())
}
