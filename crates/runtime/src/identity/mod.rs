//! Installation-key custody behind a native credential-store boundary.

use iroh::SecretKey;

mod domain;
pub use domain::{IdentityError, PublicIdentity};

pub trait KeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError>;
    fn write_secret(&self, secret: &[u8]) -> Result<(), CustodyError>;
}

/// Platform custody adapters are kept behind the identity module's seam.
mod native;
#[cfg(feature = "debug-local-profiles")]
pub use native::FileKeyCustody;
pub use native::NativeKeyCustody;

mod testing;
pub use testing::InMemoryKeyCustody;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustodyError {
    Missing,
    Unavailable,
}

/// Owns the in-memory signer without exposing its private key beyond runtime modules.
#[derive(Clone)]
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
        PublicIdentity::from_bytes(*self.secret_key.public().as_bytes())
    }

    pub(crate) fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.secret_key.sign(message).to_bytes()
    }

    pub(crate) fn transport_secret_key(&self) -> SecretKey {
        self.secret_key.clone()
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
