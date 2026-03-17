use std::path::PathBuf;

/// Allocates an IP address in the 10.20.x.y range from a counter value.
///
/// Counter 0 -> 10.20.0.0 (network), 1 -> 10.20.0.1 (bridge/host),
/// so usable sandbox IPs start at counter >= 2.
pub fn allocate_ip(counter: u32) -> String {
    let third_octet = (counter >> 8) & 0xFF;
    let fourth_octet = counter & 0xFF;
    format!("10.20.{}.{}", third_octet, fourth_octet)
}

/// Converts an IP address in the 10.20.x.y range back to a counter value.
/// Returns None if the IP is not in the expected format.
pub fn ip_to_counter(ip: &str) -> Option<u32> {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 || parts[0] != "10" || parts[1] != "20" {
        return None;
    }
    let third: u32 = parts[2].parse().ok()?;
    let fourth: u32 = parts[3].parse().ok()?;
    Some((third << 8) | fourth)
}

/// Generates a sandbox ID in the form "sbx_" + first 12 hex chars of a UUID v4.
pub fn generate_sandbox_id() -> String {
    let id = uuid::Uuid::new_v4();
    let hex = id.as_simple().to_string();
    format!("sbx_{}", &hex[..12])
}

/// Returns the directory path for a given sandbox under the data directory.
pub fn sandbox_dir(data_dir: &str, sandbox_id: &str) -> PathBuf {
    PathBuf::from(data_dir).join("sandboxes").join(sandbox_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate_ip() {
        assert_eq!(allocate_ip(2), "10.20.0.2");
        assert_eq!(allocate_ip(255), "10.20.0.255");
        assert_eq!(allocate_ip(256), "10.20.1.0");
        assert_eq!(allocate_ip(512), "10.20.2.0");
    }

    #[test]
    fn test_ip_to_counter() {
        assert_eq!(ip_to_counter("10.20.0.2"), Some(2));
        assert_eq!(ip_to_counter("10.20.0.255"), Some(255));
        assert_eq!(ip_to_counter("10.20.1.0"), Some(256));
        assert_eq!(ip_to_counter("10.20.2.0"), Some(512));
        assert_eq!(ip_to_counter("192.168.1.1"), None);
    }

    #[test]
    fn test_ip_roundtrip() {
        for counter in [2, 100, 255, 256, 1000, 65534] {
            let ip = allocate_ip(counter);
            assert_eq!(ip_to_counter(&ip), Some(counter));
        }
    }

    #[test]
    fn test_generate_sandbox_id() {
        let id = generate_sandbox_id();
        assert!(id.starts_with("sbx_"));
        assert_eq!(id.len(), 16); // "sbx_" (4) + 12 hex chars
    }

    #[test]
    fn test_sandbox_dir() {
        let dir = sandbox_dir("/var/lib/zerobox", "sbx_abc123def456");
        assert_eq!(
            dir,
            PathBuf::from("/var/lib/zerobox/sandboxes/sbx_abc123def456")
        );
    }
}
