//! Core types for snapshot and restore.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use const_hex::FromHex;
use serde::{Deserialize, Serialize};

/// BLAKE3 content hash (32 bytes).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// First 2 hex chars, used as shard directory prefix.
    pub fn prefix(&self) -> String {
        const_hex::encode(&self.0[..1])
    }

    /// Remaining 62 hex chars, used as the filename in the shard.
    pub fn suffix(&self) -> String {
        const_hex::encode(&self.0[1..])
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&const_hex::encode(self.0))
    }
}

impl fmt::Debug for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ContentHash({self})")
    }
}

impl FromStr for ContentHash {
    type Err = const_hex::FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bytes = <[u8; 32]>::from_hex(s)?;
        Ok(Self(bytes))
    }
}

impl Serialize for ContentHash {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&const_hex::encode(self.0))
    }
}

impl<'de> Deserialize<'de> for ContentHash {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// Per-file state captured at snapshot time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub hash: ContentHash,
    pub size: u64,
    pub mtime: i64,
    pub permissions: u32,
}

/// How a file changed between two snapshots.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
    PermissionsChanged,
}

impl fmt::Display for ChangeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Created => write!(f, "+"),
            Self::Modified => write!(f, "~"),
            Self::Deleted => write!(f, "-"),
            Self::PermissionsChanged => write!(f, "p"),
        }
    }
}

/// One file's change between two snapshots.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Change {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub size_delta: Option<i64>,
    pub old_hash: Option<ContentHash>,
    pub new_hash: Option<ContentHash>,
}

/// Full filesystem state at one point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotManifest {
    pub number: u32,
    pub timestamp: String,
    pub parent: Option<u32>,
    pub files: BTreeMap<PathBuf, FileState>,
    pub merkle_root: ContentHash,
}

/// One sandbox execution's snapshot session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMetadata {
    pub session_id: String,
    pub started: String,
    #[serde(default)]
    pub ended: Option<String>,
    pub command: Vec<String>,
    pub tracked_paths: Vec<PathBuf>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    pub snapshot_count: u32,
    #[serde(default)]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub merkle_roots: Vec<ContentHash>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_hex_roundtrip() {
        let bytes = [0xab; 32];
        let hash = ContentHash::from_bytes(bytes);
        let hex = hash.to_string();
        let parsed: ContentHash = hex.parse().unwrap();
        assert_eq!(hash, parsed);
    }

    #[test]
    fn content_hash_prefix_suffix() {
        let mut bytes = [0u8; 32];
        bytes[0] = 0xab;
        bytes[1] = 0xcd;
        let hash = ContentHash::from_bytes(bytes);
        assert_eq!(hash.prefix(), "ab");
        assert!(hash.suffix().starts_with("cd"));
    }

    #[test]
    fn content_hash_invalid_length() {
        assert!("abc".parse::<ContentHash>().is_err());
    }

    #[test]
    fn content_hash_invalid_hex() {
        let bad = "zz".repeat(32);
        assert!(bad.parse::<ContentHash>().is_err());
    }

    #[test]
    fn content_hash_serde_roundtrip() {
        let hash = ContentHash::from_bytes([0x42; 32]);
        let json = serde_json::to_string(&hash).unwrap();
        let parsed: ContentHash = serde_json::from_str(&json).unwrap();
        assert_eq!(hash, parsed);
    }

    #[test]
    fn change_type_display() {
        assert_eq!(ChangeType::Created.to_string(), "+");
        assert_eq!(ChangeType::Modified.to_string(), "~");
        assert_eq!(ChangeType::Deleted.to_string(), "-");
        assert_eq!(ChangeType::PermissionsChanged.to_string(), "p");
    }

    #[test]
    fn snapshot_manifest_serde_roundtrip() {
        let manifest = SnapshotManifest {
            number: 0,
            timestamp: "1234567890".to_string(),
            parent: None,
            files: BTreeMap::new(),
            merkle_root: ContentHash::from_bytes([0; 32]),
        };
        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let parsed: SnapshotManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.number, 0);
        assert!(parsed.parent.is_none());
    }
}
