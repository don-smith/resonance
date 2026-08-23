//! BLAKE3-addressed immutable content blobs.

use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

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

#[derive(Clone, Debug, Default)]
pub struct WorkspaceBlobStore {
    blobs: BTreeMap<ContentHash, Arc<[u8]>>,
    directory: Option<PathBuf>,
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

    pub(crate) fn open_durable(directory: impl AsRef<Path>) -> Result<Self, BlobError> {
        let directory = directory.as_ref().to_path_buf();
        fs::create_dir_all(&directory)?;
        let mut blobs = BTreeMap::new();
        for entry in fs::read_dir(&directory)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if !metadata.is_file() {
                return Err(BlobError::Io("blob storage contains an invalid entry"));
            }
            let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                return Err(BlobError::Io("blob storage contains an invalid entry"));
            };
            if name.starts_with(".blob-") && name.ends_with(".tmp") {
                fs::remove_file(entry.path())?;
                continue;
            }
            if !valid_hash(&name) {
                return Err(BlobError::Io("blob storage contains an invalid entry"));
            }
            let bytes = fs::read(entry.path())?;
            let hash = ContentHash(name);
            if ContentHash::from_bytes(&bytes) != hash {
                return Err(BlobError::HashMismatch);
            }
            blobs.insert(hash, bytes.into());
        }
        Ok(Self {
            blobs,
            directory: Some(directory),
        })
    }

    pub fn store(&mut self, bytes: &[u8]) -> Result<ContentHash, BlobError> {
        let hash = ContentHash::from_bytes(bytes);
        self.store_verified(&hash, bytes)?;
        Ok(hash)
    }

    pub fn open(&self, hash: &ContentHash) -> Result<Vec<u8>, BlobError> {
        self.blobs
            .get(hash)
            .map(|bytes| bytes.to_vec())
            .ok_or(BlobError::Missing)
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
        if self.blobs.contains_key(hash) {
            return Ok(());
        }
        if let Some(directory) = &self.directory {
            persist_blob(directory, hash, bytes)?;
        }
        self.blobs.insert(hash.clone(), Arc::from(bytes));
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

fn persist_blob(directory: &Path, hash: &ContentHash, bytes: &[u8]) -> Result<(), BlobError> {
    let destination = directory.join(hash.as_str());
    if destination.exists() {
        let existing = fs::read(destination)?;
        return (ContentHash::from_bytes(&existing) == *hash)
            .then_some(())
            .ok_or(BlobError::HashMismatch);
    }
    let temporary = directory.join(format!(
        ".blob-{}-{}.tmp",
        std::process::id(),
        &hash.as_str()[..12]
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, &destination)?;
        fs::File::open(directory)?.sync_all()?;
        Ok::<(), io::Error>(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(temporary);
        return Err(error.into());
    }
    Ok(())
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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

    #[test]
    fn reopens_verified_durable_bytes() {
        let directory = tempfile::tempdir().expect("blob directory creates");
        let hash = {
            let mut store =
                WorkspaceBlobStore::open_durable(directory.path()).expect("blob store opens");
            store.store(b"durable").expect("blob stores")
        };
        let reopened =
            WorkspaceBlobStore::open_durable(directory.path()).expect("blob store reopens");
        assert_eq!(reopened.open(&hash).expect("blob opens"), b"durable");
    }

    #[test]
    fn rejects_corrupt_durable_bytes_on_reopen() {
        let directory = tempfile::tempdir().expect("blob directory creates");
        let hash = {
            let mut store =
                WorkspaceBlobStore::open_durable(directory.path()).expect("blob store opens");
            store.store(b"durable").expect("blob stores")
        };
        fs::write(directory.path().join(hash.as_str()), b"corrupt").expect("durable blob corrupts");

        assert!(matches!(
            WorkspaceBlobStore::open_durable(directory.path()),
            Err(BlobError::HashMismatch)
        ));
    }

    #[test]
    fn failed_durable_write_does_not_promote_bytes_in_memory() {
        let parent = tempfile::tempdir().expect("blob parent creates");
        let directory = parent.path().join("blobs");
        let mut store = WorkspaceBlobStore::open_durable(&directory).expect("blob store opens");
        fs::remove_dir(&directory).expect("blob directory removes");
        fs::write(&directory, b"not a directory").expect("blob path becomes unavailable");
        let hash = ContentHash::from_bytes(b"uncommitted");

        assert!(matches!(
            store.store_verified(&hash, b"uncommitted"),
            Err(BlobError::Io(_))
        ));
        assert!(!store.contains(&hash));
    }

    #[test]
    fn removes_interrupted_blob_writes_before_loading_durable_bytes() {
        let directory = tempfile::tempdir().expect("blob directory creates");
        let interrupted = directory.path().join(".blob-123-abcdef.tmp");
        fs::write(&interrupted, b"partial").expect("interrupted blob writes");

        let reopened =
            WorkspaceBlobStore::open_durable(directory.path()).expect("blob store recovers");

        assert!(reopened.is_empty());
        assert!(!interrupted.exists());
    }
}
