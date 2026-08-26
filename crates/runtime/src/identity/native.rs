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
