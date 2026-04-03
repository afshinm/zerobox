//! Content-addressed object store with LZ4 compression.
//!
//! Layout: `objects/{first 2 hex chars}/{remaining 62 hex chars}`

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tempfile::NamedTempFile;

use crate::types::ContentHash;

const STREAM_THRESHOLD: u64 = 10 * 1024 * 1024;

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

    /// Hash and store a file. Large files are streamed to avoid high memory usage.
    pub fn store_file(&self, path: &Path) -> Result<ContentHash> {
        let meta = std::fs::metadata(path)
            .with_context(|| format!("failed to stat {}", path.display()))?;

        if meta.len() > STREAM_THRESHOLD {
            return self.store_file_streaming(path);
        }

        let content =
            std::fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
        let content_hash = ContentHash::from_bytes(*blake3::hash(&content).as_bytes());

        if self.has_object(&content_hash) {
            return Ok(content_hash);
        }

        self.write_object(&content_hash, &compress(&content))?;
        Ok(content_hash)
    }

    fn store_file_streaming(&self, path: &Path) -> Result<ContentHash> {
        let mut file = std::fs::File::open(path)
            .with_context(|| format!("failed to open {}", path.display()))?;

        let mut hasher = blake3::Hasher::new();
        let mut buf = [0u8; 8192];
        loop {
            let n = file
                .read(&mut buf)
                .with_context(|| format!("failed to read {}", path.display()))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        let content_hash = ContentHash::from_bytes(*hasher.finalize().as_bytes());

        if self.has_object(&content_hash) {
            return Ok(content_hash);
        }

        let shard_dir = self.objects_dir.join(content_hash.prefix());
        let obj_path = shard_dir.join(content_hash.suffix());
        std::fs::create_dir_all(&shard_dir)?;

        let mut tmp = NamedTempFile::new_in(&shard_dir)
            .with_context(|| format!("failed to create temp file in {}", shard_dir.display()))?;
        tmp.write_all(RAW_HEADER)?;

        let mut file = std::fs::File::open(path)
            .with_context(|| format!("failed to reopen {}", path.display()))?;
        std::io::copy(&mut file, &mut tmp)?;
        tmp.as_file().sync_all()?;

        match tmp.persist(&obj_path) {
            Ok(_) => Ok(content_hash),
            Err(e) if obj_path.exists() => Ok(content_hash),
            Err(e) => Err(anyhow::anyhow!(
                "failed to persist object {content_hash}: {}",
                e.error
            )),
        }
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

    #[test]
    fn large_file_streamed_and_retrievable() {
        let (dir, store) = setup();
        let path = dir.path().join("large.bin");
        let data = vec![0x42u8; (STREAM_THRESHOLD as usize) + 1];
        std::fs::write(&path, &data).unwrap();

        let hash = store.store_file(&path).unwrap();
        let retrieved = store.retrieve(&hash).unwrap();
        assert_eq!(retrieved.len(), data.len());
        assert_eq!(retrieved, data);
    }

    #[test]
    fn retrieve_nonexistent_hash_fails() {
        let (_dir, store) = setup();
        let fake = ContentHash::from_bytes([0xaa; 32]);
        assert!(store.retrieve(&fake).is_err());
    }

    #[test]
    fn store_empty_file() {
        let (dir, store) = setup();
        let path = dir.path().join("empty.txt");
        std::fs::write(&path, b"").unwrap();
        let hash = store.store_file(&path).unwrap();
        let retrieved = store.retrieve(&hash).unwrap();
        assert!(retrieved.is_empty());
    }

    #[test]
    fn truncated_blob_detected() {
        let (_dir, store) = setup();
        let hash = store.store_bytes(b"good data").unwrap();
        let obj_path = store.object_path(&hash);
        std::fs::write(&obj_path, b"XX").unwrap();
        assert!(store.retrieve(&hash).is_err());
    }
}
