//! Networking integration tests

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_sandbox_has_network() {
        // TODO: Create sandbox, run "ping -c 1 8.8.8.8", verify success
    }

    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_port_forwarding() {
        // TODO: Create sandbox with port, start HTTP server inside, curl from host
    }

    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_network_isolation() {
        // TODO: Create two sandboxes, verify they can't reach each other directly
    }
}
