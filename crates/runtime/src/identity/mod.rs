//! Installation-key custody behind a native credential-store boundary.

use std::{fmt, sync::Mutex};

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
