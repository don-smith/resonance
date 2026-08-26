use std::sync::Mutex;

use super::{CustodyError, KeyCustody};

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
