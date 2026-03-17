//! Sandbox lifecycle integration tests
//! Requires a running zerobox daemon and /dev/kvm access

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_create_and_destroy_sandbox() {
        // TODO: Create sandbox via API, verify status, destroy
    }

    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_create_stop_start_sandbox() {
        // TODO: Create, stop, verify stopped status
    }

    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_list_sandboxes() {
        // TODO: Create multiple sandboxes, list, verify count
    }

    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_sandbox_timeout() {
        // TODO: Create with short timeout, verify auto-stop
    }
}
