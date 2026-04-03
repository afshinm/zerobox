//! Content-addressed object store with LZ4 compression.
//!
//! Layout: `objects/{first 2 hex chars}/{remaining 62 hex chars}`

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

use crate::types::ContentHash;

const LZ4_HEADER: &[u8; 4] = b"LZ4T";
const RAW_HEADER: &[u8; 4] = &[0, 0, 0, 0];
const COMPRESS_THRESHOLD: usize = 64;
pub struct ObjectStore {
    objects_dir: PathBuf,
}

impl ObjectStore {
    pub fn new(root: PathBuf) -> Result<Self> {
        let objects_dir = root.join("objects");
        std::fs::create_dir_all(&objects_dir)
            .with_context(|| format!("failed to create objects dir {}", objects_dir.display()))?;
        Ok(Self { objects_dir })
    }

    /// Hash, compress, and store a file. Reads once to avoid TOCTOU.
    /// Skips the write if identical content already exists.
    pub fn store_file(&self, path: &Path) -> Result<ContentHash> {
        let content =
            std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        let content_hash = ContentHash::from_bytes(*blake3::hash(&content).as_bytes());

        if self.has_object(&content_hash) {
            return Ok(content_hash);
        }

        self.write_object(&content_hash, &compress(&content))?;
        Ok(content_hash)
    }

    /// Store raw bytes. Returns the content hash.
    pub fn store_bytes(&self, content: &[u8]) -> Result<ContentHash> {
        let content_hash = ContentHash::from_bytes(*blake3::hash(content).as_bytes());

        if self.has_object(&content_hash) {
            return Ok(content_hash);
        }

        self.write_object(&content_hash, &compress(content))?;
        Ok(content_hash)
    }

    /// Retrieve and decompress content by hash. Verifies integrity on every read.
    pub fn retrieve(&self, hash: &ContentHash) -> Result<Vec<u8>> {
        let obj_path = self.object_path(hash);
        let blob = std::fs::read(&obj_path).with_context(|| format!("object not found: {hash}"))?;
        let content = decompress(&blob)?;

        let actual = ContentHash::from_bytes(*blake3::hash(&content).as_bytes());
        if actual != *hash {
            anyhow::bail!("integrity check failed for {hash}: stored content hashes to {actual}");
        }

        Ok(content)
    }

    /// Retrieve content and write it atomically to a target path.
    pub fn retrieve_to(&self, hash: &ContentHash, target: &Path) -> Result<()> {
        let content = self.retrieve(hash)?;
        let parent = target
            .parent()
            .ok_or_else(|| anyhow::anyhow!("no parent for {}", target.display()))?;
        std::fs::create_dir_all(parent)?;

        let mut tmp = NamedTempFile::new_in(parent)
            .with_context(|| format!("failed to create temp file in {}", parent.display()))?;
        tmp.write_all(&content)?;
        tmp.persist(target).map_err(|e| {
            anyhow::anyhow!("failed to persist to {}: {}", target.display(), e.error)
        })?;

        Ok(())
    }

    pub fn has_object(&self, hash: &ContentHash) -> bool {
        self.object_path(hash).exists()
    }

    /// Re-reads and re-hashes stored content to detect corruption.
    pub fn verify(&self, hash: &ContentHash) -> Result<bool> {
        match self.retrieve(hash) {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    fn object_path(&self, hash: &ContentHash) -> PathBuf {
        self.objects_dir.join(hash.prefix()).join(hash.suffix())
    }

    /// Write a blob atomically. If another process races us on the same hash,
    /// both produce identical content, so whoever wins is fine.
    fn write_object(&self, hash: &ContentHash, blob: &[u8]) -> Result<()> {
        let shard_dir = self.objects_dir.join(hash.prefix());
        let obj_path = shard_dir.join(hash.suffix());

        std::fs::create_dir_all(&shard_dir)?;

        let mut tmp = NamedTempFile::new_in(&shard_dir)
            .with_context(|| format!("failed to create temp file in {}", shard_dir.display()))?;
        tmp.write_all(blob)?;
        tmp.as_file().sync_all()?;

        match tmp.persist(&obj_path) {
            Ok(_) => Ok(()),
            Err(e) => {
                if obj_path.exists() {
                    Ok(())
                } else {
                    Err(anyhow::anyhow!(
                        "failed to persist object {hash}: {}",
                        e.error
                    ))
                }
            }
        }
    }
}

/// LZ4 compress with a 4-byte header. Stores raw if the blob is tiny or incompressible.
fn compress(content: &[u8]) -> Vec<u8> {
    if content.len() < COMPRESS_THRESHOLD {
        let mut blob = Vec::with_capacity(4 + content.len());
        blob.extend_from_slice(RAW_HEADER);
        blob.extend_from_slice(content);
        return blob;
    }

    let compressed = lz4_flex::compress_prepend_size(content);
    if compressed.len() >= content.len() {
        let mut blob = Vec::with_capacity(4 + content.len());
        blob.extend_from_slice(RAW_HEADER);
        blob.extend_from_slice(content);
        blob
    } else {
        let mut blob = Vec::with_capacity(4 + compressed.len());
        blob.extend_from_slice(LZ4_HEADER);
        blob.extend_from_slice(&compressed);
        blob
    }
}

/// Decompress a blob based on its 4-byte header.
fn decompress(blob: &[u8]) -> Result<Vec<u8>> {
    anyhow::ensure!(blob.len() >= 4, "blob too short");
    let header = &blob[..4];
    let data = &blob[4..];
    if header == LZ4_HEADER {
        lz4_flex::decompress_size_prepended(data)
            .map_err(|e| anyhow::anyhow!("LZ4 decompression failed: {e}"))
    } else {
        Ok(data.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, ObjectStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = ObjectStore::new(dir.path().join("store")).unwrap();
        (dir, store)
    }

    #[test]
    fn store_and_retrieve_bytes() {
        let (_dir, store) = setup();
        let data = b"hello world";
        let hash = store.store_bytes(data).unwrap();
        let retrieved = store.retrieve(&hash).unwrap();
        assert_eq!(retrieved, data);
    }

    #[test]
    fn store_file_roundtrip() {
        let (dir, store) = setup();
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, "file content").unwrap();
        let hash = store.store_file(&file_path).unwrap();
        let retrieved = store.retrieve(&hash).unwrap();
        assert_eq!(retrieved, b"file content");
    }

    #[test]
    fn deduplication() {
        let (_dir, store) = setup();
        let data = b"duplicate content";
        let hash1 = store.store_bytes(data).unwrap();
        let hash2 = store.store_bytes(data).unwrap();
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn retrieve_to_writes_file() {
        let (dir, store) = setup();
        let data = b"restore me";
        let hash = store.store_bytes(data).unwrap();
        let target = dir.path().join("restored.txt");
        store.retrieve_to(&hash, &target).unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), data);
    }

    #[test]
    fn verify_integrity() {
        let (_dir, store) = setup();
        let hash = store.store_bytes(b"verify this").unwrap();
        assert!(store.verify(&hash).unwrap());
    }

    #[test]
    fn has_object_true_after_store() {
        let (_dir, store) = setup();
        let hash = store.store_bytes(b"exists").unwrap();
        assert!(store.has_object(&hash));
    }

    #[test]
    fn has_object_false_for_missing() {
        let (_dir, store) = setup();
        let hash = ContentHash::from_bytes([0xff; 32]);
        assert!(!store.has_object(&hash));
    }

    #[test]
    fn compression_roundtrip() {
        let data = b"hello world this is a test of lz4 compression with enough data to compress";
        let compressed = compress(data);
        let decompressed = decompress(&compressed).unwrap();
        assert_eq!(decompressed, data);
    }

    #[test]
    fn tiny_blobs_stored_raw() {
        let data = b"tiny";
        let blob = compress(data);
        assert_eq!(&blob[..4], RAW_HEADER);
    }

    #[test]
    fn sharded_directory_layout() {
        let (_dir, store) = setup();
        let hash = store.store_bytes(b"check layout").unwrap();
        let obj_path = store.object_path(&hash);
        assert!(obj_path.exists());
        let parent = obj_path.parent().unwrap();
        assert_eq!(parent.file_name().unwrap().len(), 2);
    }
}
