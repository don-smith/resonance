//! Installation-key custody behind a native credential-store boundary.

use std::{fmt, sync::Mutex};

#[cfg(feature = "debug-local-profiles")]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use iroh::SecretKey;

const KEYCHAIN_SERVICE: &str = "dev.resonance.desktop";
const KEYCHAIN_ACCOUNT: &str = "installation-identity";

pub trait KeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError>;
    fn write_secret(&self, secret: &[u8]) -> Result<(), CustodyError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyError {
    Missing,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum IdentityError {
    MalformedStoredSecret,
    StoreUnavailable,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedStoredSecret => formatter.write_str(
                "the installation identity in the native credential store is malformed; remove it only after recovering the installation identity",
            ),
            Self::StoreUnavailable => formatter.write_str(
                "the native credential store is unavailable; unlock or repair it before using Resonance",
            ),
        }
    }
}

impl std::error::Error for IdentityError {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct PublicIdentity([u8; 32]);

impl PublicIdentity {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for PublicIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("PublicIdentity")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for PublicIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Owns the in-memory signer without exposing its private key beyond runtime modules.
pub struct InstallationIdentity {
    secret_key: SecretKey,
}

impl InstallationIdentity {
    pub fn load_or_create(custody: &impl KeyCustody) -> Result<Self, IdentityError> {
        match custody.read_secret() {
            Ok(bytes) => {
                let bytes: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| IdentityError::MalformedStoredSecret)?;
                Ok(Self {
                    secret_key: SecretKey::from_bytes(&bytes),
                })
            }
            Err(CustodyError::Missing) => {
                let secret_key = SecretKey::generate();
                custody
                    .write_secret(&secret_key.to_bytes())
                    .map_err(|_| IdentityError::StoreUnavailable)?;
                Ok(Self { secret_key })
            }
            Err(CustodyError::Unavailable) => Err(IdentityError::StoreUnavailable),
        }
    }

    pub fn load_or_create_native() -> Result<Self, IdentityError> {
        Self::load_or_create(&NativeKeyCustody::open()?)
    }

    #[must_use]
    pub fn public_identity(&self) -> PublicIdentity {
        PublicIdentity(*self.secret_key.public().as_bytes())
    }

    pub(crate) fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.secret_key.sign(message).to_bytes()
    }

    pub(crate) fn transport_secret_key(&self) -> SecretKey {
        self.secret_key.clone()
    }
}

pub struct NativeKeyCustody {
    entry: keyring::Entry,
}

impl NativeKeyCustody {
    pub fn open() -> Result<Self, IdentityError> {
        keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
            .map(|entry| Self { entry })
            .map_err(|_| IdentityError::StoreUnavailable)
    }
}

impl KeyCustody for NativeKeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError> {
        self.entry.get_secret().map_err(|error| match error {
            keyring::Error::NoEntry => CustodyError::Missing,
            _ => CustodyError::Unavailable,
        })
    }

    fn write_secret(&self, secret: &[u8]) -> Result<(), CustodyError> {
        self.entry
            .set_secret(secret)
            .map_err(|_| CustodyError::Unavailable)
    }
}

/// Debug-only, owner-only file custody for the desktop profile launcher.
///
/// It is deliberately not available in a normal runtime build. The caller owns
/// the profile lock; this adapter owns only a fixed key file inside the already
/// validated identity directory.
#[cfg(feature = "debug-local-profiles")]
pub struct FileKeyCustody {
    key_file: PathBuf,
}

#[cfg(feature = "debug-local-profiles")]
impl FileKeyCustody {
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, CustodyError> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            let directory = directory.as_ref();
            fs::create_dir_all(directory).map_err(|_| CustodyError::Unavailable)?;
            let metadata =
                fs::symlink_metadata(directory).map_err(|_| CustodyError::Unavailable)?;
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(CustodyError::Unavailable);
            }
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
                .map_err(|_| CustodyError::Unavailable)?;
            Ok(Self {
                key_file: directory.join("installation.key"),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = directory;
            Err(CustodyError::Unavailable)
        }
    }

    fn temporary_file(&self) -> PathBuf {
        static NEXT_TEMPORARY_FILE: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT_TEMPORARY_FILE.fetch_add(1, Ordering::Relaxed);
        self.key_file.with_file_name(format!(
            ".installation.key.{}.{}.tmp",
            std::process::id(),
            sequence
        ))
    }
}

#[cfg(all(feature = "debug-local-profiles", unix))]
impl KeyCustody for FileKeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = match fs::symlink_metadata(&self.key_file) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(CustodyError::Missing);
            }
            Err(_) => return Err(CustodyError::Unavailable),
        };
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(CustodyError::Unavailable);
        }
        fs::read(&self.key_file).map_err(|_| CustodyError::Unavailable)
    }

    fn write_secret(&self, secret: &[u8]) -> Result<(), CustodyError> {
        use std::os::unix::fs::OpenOptionsExt;

        if self.key_file.exists() {
            return Err(CustodyError::Unavailable);
        }
        let temporary_file = self.temporary_file();
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary_file)?;
            file.write_all(secret)?;
            file.sync_all()?;
            // Creating a hard link is atomic and refuses to overwrite a key
            // published by a competing process. The profile lock normally
            // prevents this race; this remains fail-closed if it is violated.
            fs::hard_link(&temporary_file, &self.key_file)?;
            fs::remove_file(&temporary_file)?;
            OpenOptions::new()
                .read(true)
                .open(
                    self.key_file
                        .parent()
                        .ok_or_else(|| std::io::Error::other("key file has no parent"))?,
                )?
                .sync_all()?;
            Ok::<(), std::io::Error>(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary_file);
            return Err(CustodyError::Unavailable);
        }
        Ok(())
    }
}

#[cfg(all(feature = "debug-local-profiles", not(unix)))]
impl KeyCustody for FileKeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError> {
        Err(CustodyError::Unavailable)
    }

    fn write_secret(&self, _secret: &[u8]) -> Result<(), CustodyError> {
        Err(CustodyError::Unavailable)
    }
}

#[derive(Default)]
pub struct InMemoryKeyCustody {
    state: Mutex<InMemoryState>,
}

#[derive(Default)]
struct InMemoryState {
    secret: Option<Vec<u8>>,
    fail_read: bool,
    fail_write: bool,
}

impl InMemoryKeyCustody {
    #[must_use]
    pub fn with_secret(secret: Vec<u8>) -> Self {
        Self {
            state: Mutex::new(InMemoryState {
                secret: Some(secret),
                ..InMemoryState::default()
            }),
        }
    }

    #[must_use]
    pub fn failing_read() -> Self {
        Self {
            state: Mutex::new(InMemoryState {
                fail_read: true,
                ..InMemoryState::default()
            }),
        }
    }

    #[must_use]
    pub fn failing_write() -> Self {
        Self {
            state: Mutex::new(InMemoryState {
                fail_write: true,
                ..InMemoryState::default()
            }),
        }
    }

    #[must_use]
    pub fn stored_secret_len(&self) -> Option<usize> {
        self.state
            .lock()
            .expect("in-memory custody lock must not be poisoned")
            .secret
            .as_ref()
            .map(Vec::len)
    }
}

impl KeyCustody for InMemoryKeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError> {
        let state = self.state.lock().map_err(|_| CustodyError::Unavailable)?;
        if state.fail_read {
            Err(CustodyError::Unavailable)
        } else {
            state.secret.clone().ok_or(CustodyError::Missing)
        }
    }

    fn write_secret(&self, secret: &[u8]) -> Result<(), CustodyError> {
        let mut state = self.state.lock().map_err(|_| CustodyError::Unavailable)?;
        if state.fail_write {
            Err(CustodyError::Unavailable)
        } else {
            state.secret = Some(secret.to_vec());
            Ok(())
        }
    }
}

#[cfg(all(test, feature = "debug-local-profiles", unix))]
mod file_custody_tests {
    use std::{fs, os::unix::fs::PermissionsExt};

    use super::{CustodyError, FileKeyCustody, InstallationIdentity, KeyCustody};

    #[test]
    fn atomically_creates_an_owner_only_stable_key() {
        let directory = tempfile::tempdir().expect("temporary directory creates");
        let custody = FileKeyCustody::open(directory.path()).expect("custody opens");
        let first = InstallationIdentity::load_or_create(&custody).expect("identity creates");
        let second = InstallationIdentity::load_or_create(&custody).expect("identity reloads");

        assert_eq!(first.public_identity(), second.public_identity());
        let key = directory.path().join("installation.key");
        assert_eq!(
            fs::metadata(key)
                .expect("key metadata")
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }

    #[test]
    fn malformed_or_inaccessible_key_never_generates_a_replacement() {
        let directory = tempfile::tempdir().expect("temporary directory creates");
        let custody = FileKeyCustody::open(directory.path()).expect("custody opens");
        let key = directory.path().join("installation.key");
        fs::write(&key, [7; 31]).expect("malformed key writes");
        fs::set_permissions(&key, fs::Permissions::from_mode(0o600)).expect("mode sets");
        assert!(InstallationIdentity::load_or_create(&custody).is_err());
        assert_eq!(fs::read(&key).expect("key remains readable"), [7; 31]);

        fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).expect("mode loosens");
        assert_eq!(custody.read_secret(), Err(CustodyError::Unavailable));
    }
}
