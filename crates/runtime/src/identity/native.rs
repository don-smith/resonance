#[cfg(feature = "debug-local-profiles")]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use super::{CustodyError, IdentityError, KeyCustody};

const KEYCHAIN_SERVICE: &str = "dev.resonance.desktop";
const KEYCHAIN_ACCOUNT: &str = "installation-identity";
const RECIPIENT_KEYCHAIN_ACCOUNT: &str = "conversation-recipient-key";
const RECIPIENT_MARKER_KEYCHAIN_ACCOUNT: &str = "conversation-recipient-key-created";

pub struct NativeKeyCustody {
    entry: keyring::Entry,
    recipient_entry: keyring::Entry,
    recipient_marker: keyring::Entry,
}

impl NativeKeyCustody {
    pub fn open() -> Result<Self, IdentityError> {
        Ok(Self {
            entry: keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
                .map_err(|_| IdentityError::StoreUnavailable)?,
            recipient_entry: keyring::Entry::new(KEYCHAIN_SERVICE, RECIPIENT_KEYCHAIN_ACCOUNT)
                .map_err(|_| IdentityError::StoreUnavailable)?,
            recipient_marker: keyring::Entry::new(
                KEYCHAIN_SERVICE,
                RECIPIENT_MARKER_KEYCHAIN_ACCOUNT,
            )
            .map_err(|_| IdentityError::StoreUnavailable)?,
        })
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

    fn read_recipient_secret(&self) -> Result<Vec<u8>, CustodyError> {
        self.recipient_entry
            .get_secret()
            .map_err(|error| match error {
                keyring::Error::NoEntry => CustodyError::Missing,
                _ => CustodyError::Unavailable,
            })
    }

    fn write_recipient_secret(&self, secret: &[u8]) -> Result<(), CustodyError> {
        self.recipient_entry
            .set_secret(secret)
            .map_err(|_| CustodyError::Unavailable)
    }

    fn recipient_secret_was_created(&self) -> Result<bool, CustodyError> {
        match self.recipient_marker.get_secret() {
            Ok(marker) if marker == b"created" => Ok(true),
            Ok(_) => Err(CustodyError::Unavailable),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(_) => Err(CustodyError::Unavailable),
        }
    }

    fn mark_recipient_secret_created(&self) -> Result<(), CustodyError> {
        self.recipient_marker
            .set_secret(b"created")
            .map_err(|_| CustodyError::Unavailable)
    }
}

/// Debug-only, owner-only file custody for the desktop profile launcher.
#[cfg(feature = "debug-local-profiles")]
pub struct FileKeyCustody {
    key_file: PathBuf,
    recipient_key_file: PathBuf,
    recipient_marker_file: PathBuf,
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
                recipient_key_file: directory.join("conversation-recipient.key"),
                recipient_marker_file: directory.join("conversation-recipient.created"),
            })
        }
        #[cfg(not(unix))]
        {
            let _ = directory;
            Err(CustodyError::Unavailable)
        }
    }

    fn temporary_file(key_file: &Path) -> PathBuf {
        static NEXT_TEMPORARY_FILE: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT_TEMPORARY_FILE.fetch_add(1, Ordering::Relaxed);
        key_file.with_file_name(format!(
            ".{}.{}.{}.tmp",
            key_file
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("key"),
            std::process::id(),
            sequence
        ))
    }

    #[cfg(unix)]
    fn read_file(path: &Path) -> Result<Vec<u8>, CustodyError> {
        use std::os::unix::fs::PermissionsExt;

        let metadata = match fs::symlink_metadata(path) {
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
        fs::read(path).map_err(|_| CustodyError::Unavailable)
    }

    #[cfg(unix)]
    fn write_file(path: &Path, secret: &[u8]) -> Result<(), CustodyError> {
        use std::os::unix::fs::OpenOptionsExt;

        if path.exists() {
            return Err(CustodyError::Unavailable);
        }
        let temporary_file = Self::temporary_file(path);
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary_file)?;
            file.write_all(secret)?;
            file.sync_all()?;
            fs::hard_link(&temporary_file, path)?;
            fs::remove_file(&temporary_file)?;
            OpenOptions::new()
                .read(true)
                .open(
                    path.parent()
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

#[cfg(all(feature = "debug-local-profiles", unix))]
impl KeyCustody for FileKeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError> {
        Self::read_file(&self.key_file)
    }

    fn write_secret(&self, secret: &[u8]) -> Result<(), CustodyError> {
        Self::write_file(&self.key_file, secret)
    }

    fn read_recipient_secret(&self) -> Result<Vec<u8>, CustodyError> {
        Self::read_file(&self.recipient_key_file)
    }

    fn write_recipient_secret(&self, secret: &[u8]) -> Result<(), CustodyError> {
        Self::write_file(&self.recipient_key_file, secret)
    }

    fn recipient_secret_was_created(&self) -> Result<bool, CustodyError> {
        match Self::read_file(&self.recipient_marker_file) {
            Ok(marker) if marker == b"created" => Ok(true),
            Ok(_) => Err(CustodyError::Unavailable),
            Err(CustodyError::Missing) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn mark_recipient_secret_created(&self) -> Result<(), CustodyError> {
        match Self::write_file(&self.recipient_marker_file, b"created") {
            Ok(()) => Ok(()),
            Err(CustodyError::Unavailable) if self.recipient_marker_file.exists() => {
                self.recipient_secret_was_created().and_then(|created| {
                    if created {
                        Ok(())
                    } else {
                        Err(CustodyError::Unavailable)
                    }
                })
            }
            Err(error) => Err(error),
        }
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

    fn read_recipient_secret(&self) -> Result<Vec<u8>, CustodyError> {
        Err(CustodyError::Unavailable)
    }

    fn write_recipient_secret(&self, _secret: &[u8]) -> Result<(), CustodyError> {
        Err(CustodyError::Unavailable)
    }

    fn recipient_secret_was_created(&self) -> Result<bool, CustodyError> {
        Err(CustodyError::Unavailable)
    }

    fn mark_recipient_secret_created(&self) -> Result<(), CustodyError> {
        Err(CustodyError::Unavailable)
    }
}
