//! Merkle tree for cryptographic filesystem state commitment (RFC 6962).

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::Result;
use rs_merkle::{Hasher, MerkleTree};

use crate::types::{ContentHash, FileState};

/// BLAKE3 hasher with domain separation per RFC 6962.
#[derive(Clone)]
pub struct Blake3Rfc6962;

const LEAF_PREFIX: u8 = 0x00;
const INTERNAL_PREFIX: u8 = 0x01;

impl Hasher for Blake3Rfc6962 {
    type Hash = [u8; 32];

    fn hash(data: &[u8]) -> [u8; 32] {
        *blake3::hash(data).as_bytes()
    }

    fn concat_and_hash(left: &Self::Hash, right: Option<&Self::Hash>) -> Self::Hash {
        match right {
            Some(r) => {
                let mut hasher = blake3::Hasher::new();
                hasher.update(&[INTERNAL_PREFIX]);
                hasher.update(left);
                hasher.update(r);
                *hasher.finalize().as_bytes()
            }
            None => *left,
        }
    }
}

/// Leaf = BLAKE3(0x00 || path || content_hash). Binds file path to its content.
fn compute_leaf(path: &str, content_hash: &ContentHash) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&[LEAF_PREFIX]);
    hasher.update(path.as_bytes());
    hasher.update(content_hash.as_bytes());
    *hasher.finalize().as_bytes()
}

/// Compute the merkle root over all files in a manifest.
/// Deterministic: paths are sorted (BTreeMap) before tree construction.
pub fn merkle_root(files: &BTreeMap<PathBuf, FileState>) -> Result<ContentHash> {
    if files.is_empty() {
        return Ok(ContentHash::from_bytes(*blake3::hash(b"").as_bytes()));
    }
    let leaves: Vec<[u8; 32]> = files
        .iter()
        .map(|(path, state)| {
            let path_str = path.to_string_lossy();
            compute_leaf(&path_str, &state.hash)
        })
        .collect();

    let tree = MerkleTree::<Blake3Rfc6962>::from_leaves(&leaves);
    let root = tree
        .root()
        .ok_or_else(|| anyhow::anyhow!("failed to compute merkle root"))?;

    Ok(ContentHash::from_bytes(root))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FileState;

    fn make_state(hash_byte: u8) -> FileState {
        FileState {
            hash: ContentHash::from_bytes([hash_byte; 32]),
            size: 100,
            mtime: 1000,
            permissions: 0o644,
        }
    }

    #[test]
    fn empty_tree_has_deterministic_root() {
        let files = BTreeMap::new();
        let root1 = merkle_root(&files).unwrap();
        let root2 = merkle_root(&files).unwrap();
        assert_eq!(root1, root2);
    }

    #[test]
    fn single_file_tree() {
        let mut files = BTreeMap::new();
        files.insert(PathBuf::from("a.txt"), make_state(0x01));
        let root = merkle_root(&files).unwrap();
        let expected = compute_leaf("a.txt", &ContentHash::from_bytes([0x01; 32]));
        assert_eq!(*root.as_bytes(), expected);
    }

    #[test]
    fn root_changes_when_content_changes() {
        let mut files1 = BTreeMap::new();
        files1.insert(PathBuf::from("a.txt"), make_state(0x01));
        let mut files2 = BTreeMap::new();
        files2.insert(PathBuf::from("a.txt"), make_state(0x02));
        assert_ne!(merkle_root(&files1).unwrap(), merkle_root(&files2).unwrap());
    }

    #[test]
    fn root_changes_when_path_changes() {
        let mut files1 = BTreeMap::new();
        files1.insert(PathBuf::from("a.txt"), make_state(0x01));
        let mut files2 = BTreeMap::new();
        files2.insert(PathBuf::from("b.txt"), make_state(0x01));
        assert_ne!(merkle_root(&files1).unwrap(), merkle_root(&files2).unwrap());
    }

    #[test]
    fn deterministic_regardless_of_insertion_order() {
        let mut files1 = BTreeMap::new();
        files1.insert(PathBuf::from("b.txt"), make_state(0x02));
        files1.insert(PathBuf::from("a.txt"), make_state(0x01));

        let mut files2 = BTreeMap::new();
        files2.insert(PathBuf::from("a.txt"), make_state(0x01));
        files2.insert(PathBuf::from("b.txt"), make_state(0x02));

        assert_eq!(merkle_root(&files1).unwrap(), merkle_root(&files2).unwrap());
    }

    #[test]
    fn adding_file_changes_root() {
        let mut files1 = BTreeMap::new();
        files1.insert(PathBuf::from("a.txt"), make_state(0x01));
        let mut files2 = files1.clone();
        files2.insert(PathBuf::from("b.txt"), make_state(0x02));
        assert_ne!(merkle_root(&files1).unwrap(), merkle_root(&files2).unwrap());
    }
}
