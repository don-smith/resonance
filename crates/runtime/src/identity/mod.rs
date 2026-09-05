//! Installation-key custody behind a native credential-store boundary.

use commonware_codec::DecodeExt;
use commonware_cryptography::{ed25519, Signer as _};
use iroh::SecretKey;

mod domain;
pub use domain::{IdentityError, PublicIdentity};

pub trait KeyCustody {
    fn read_secret(&self) -> Result<Vec<u8>, CustodyError>;
    fn write_secret(&self, secret: &[u8]) -> Result<(), CustodyError>;
}

/// Platform custody adapters are kept behind the identity module's seam.
/// Native credential-store adapters retain the historical public namespace.
pub mod native;
#[cfg(feature = "debug-local-profiles")]
pub use native::FileKeyCustody;
pub use native::NativeKeyCustody;

/// Test custody is explicit while its implementation remains separate.
pub mod testing;
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
                Self::from_secret_key(SecretKey::from_bytes(&bytes))
            }
            Err(CustodyError::Missing) => {
                let secret_key = SecretKey::generate();
                custody
                    .write_secret(&secret_key.to_bytes())
                    .map_err(|_| IdentityError::StoreUnavailable)?;
                Self::from_secret_key(secret_key)
            }
            Err(CustodyError::Unavailable) => Err(IdentityError::StoreUnavailable),
        }
    }

    fn from_secret_key(secret_key: SecretKey) -> Result<Self, IdentityError> {
        let identity = Self { secret_key };
        identity.commonware_signer()?;
        Ok(identity)
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

    /// Reconstructs Commonware's signer from the installation secret already in memory.
    /// No second signing secret is generated or persisted.
    pub(crate) fn commonware_signer(&self) -> Result<ed25519::PrivateKey, IdentityError> {
        let secret = self.secret_key.to_bytes();
        let signer = ed25519::PrivateKey::decode(secret.as_slice())
            .map_err(|_| IdentityError::MalformedStoredSecret)?;
        if signer.public_key().as_ref() != self.public_identity().as_bytes() {
            return Err(IdentityError::MalformedStoredSecret);
        }
        Ok(signer)
    }
}

#[cfg(test)]
mod commonware_identity_tests {
    use commonware_cryptography::{Signer as _, Verifier as _};

    use super::{InMemoryKeyCustody, InstallationIdentity};

    #[test]
    fn reconstructs_commonware_signer_from_the_installation_secret() {
        let custody = InMemoryKeyCustody::with_secret(vec![41; 32]);
        let identity = InstallationIdentity::load_or_create(&custody).expect("identity loads");

        let signer = identity
            .commonware_signer()
            .expect("commonware signer reconstructs");
        let signature = signer.sign(b"resonance.identity.test", b"same key");

        assert_eq!(
            signer.public_key().as_ref(),
            identity.public_identity().as_bytes()
        );
        assert!(signer
            .public_key()
            .verify(b"resonance.identity.test", b"same key", &signature));
        assert_eq!(custody.stored_secret_len(), Some(32));
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
