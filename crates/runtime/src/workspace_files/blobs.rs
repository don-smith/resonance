//! BLAKE3-addressed immutable content blobs.

use std::{collections::BTreeMap, io};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContentHash(pub String);

impl ContentHash {
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let hash = blake3::hash(bytes);
        Self(hash.to_hex().to_string())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Default)]
pub struct WorkspaceBlobStore {
    blobs: BTreeMap<ContentHash, Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BlobError {
    Missing,
    HashMismatch,
    Io(/* message */ &'static str),
}

impl std::fmt::Display for BlobError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => formatter.write_str("blob is not available"),
            Self::HashMismatch => {
                formatter.write_str("blob content does not match its declared hash")
            }
            Self::Io(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for BlobError {}

impl WorkspaceBlobStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn store(&mut self, bytes: &[u8]) -> Result<ContentHash, BlobError> {
        let hash = ContentHash::from_bytes(bytes);
        self.blobs
            .entry(hash.clone())
            .or_insert_with(|| bytes.to_vec());
        Ok(hash)
    }

    pub fn open(&self, hash: &ContentHash) -> Result<Vec<u8>, BlobError> {
        self.blobs.get(hash).cloned().ok_or(BlobError::Missing)
    }

    pub fn verify(&self, hash: &ContentHash, bytes: &[u8]) -> Result<(), BlobError> {
        let actual = ContentHash::from_bytes(bytes);
        if actual == *hash {
            Ok(())
        } else {
            Err(BlobError::HashMismatch)
        }
    }

    pub fn store_verified(&mut self, hash: &ContentHash, bytes: &[u8]) -> Result<(), BlobError> {
        self.verify(hash, bytes)?;
        self.blobs
            .entry(hash.clone())
            .or_insert_with(|| bytes.to_vec());
        Ok(())
    }

    #[must_use]
    pub fn contains(&self, hash: &ContentHash) -> bool {
        self.blobs.contains_key(hash)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.blobs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.blobs.is_empty()
    }
}

impl From<io::Error> for BlobError {
    fn from(_error: io::Error) -> Self {
        Self::Io("blob I/O failed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_and_retrieves_verified_bytes() {
        let mut store = WorkspaceBlobStore::new();
        let hash = store.store(b"hello").expect("stores");
        let bytes = store.open(&hash).expect("opens");
        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn rejects_mismatched_hash_in_verified_store() {
        let mut store = WorkspaceBlobStore::new();
        let hash = ContentHash::from_bytes(b"correct");
        assert!(store.store_verified(&hash, b"wrong").is_err());
    }

    #[test]
    fn same_content_produces_same_hash() {
        let hash1 = ContentHash::from_bytes(b"same");
        let hash2 = ContentHash::from_bytes(b"same");
        assert_eq!(hash1, hash2);
    }
}
