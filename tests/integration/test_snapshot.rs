//! Snapshot integration tests

#[cfg(test)]
mod tests {
    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_create_snapshot() {
        // TODO: Create sandbox, create snapshot, verify metadata
    }

    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_restore_from_snapshot() {
        // TODO: Create sandbox, write file, snapshot, restore, verify file exists
    }

    #[test]
    #[ignore = "requires running daemon with KVM"]
    fn test_list_and_delete_snapshots() {
        // TODO: Create snapshots, list, delete, verify
    }
}
