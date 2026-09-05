//! Canonical conversation records and cryptography kept behind the runtime boundary.

pub mod authority;
mod channels;
pub mod crypto;
mod key_custody;
mod recovery;
pub mod runtime;
mod store;
pub mod testing;
pub mod wire;

use std::fmt;

/// Finite refusal reasons at the conversation wire and cryptography boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationError {
    MalformedBytes(&'static str),
    UnsupportedFamily(u8),
    UnsupportedVersion { family: u8, version: u8 },
    UnsupportedSuite(u8),
    SizeLimit { field: &'static str, limit: usize },
    UnauthorizedData(&'static str),
    MissingKey,
    RecipientMismatch,
    InvalidSignature,
    AuthenticatedOpen,
    LocalSealingFailure,
    RandomnessUnavailable,
}

impl fmt::Display for ConversationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedBytes(reason) => {
                write!(formatter, "malformed conversation bytes: {reason}")
            }
            Self::UnsupportedFamily(family) => {
                write!(formatter, "unsupported conversation family {family}")
            }
            Self::UnsupportedVersion { family, version } => {
                write!(
                    formatter,
                    "unsupported version {version} for family {family}"
                )
            }
            Self::UnsupportedSuite(suite) => {
                write!(formatter, "unsupported conversation suite {suite}")
            }
            Self::SizeLimit { field, limit } => {
                write!(formatter, "conversation {field} exceeds limit {limit}")
            }
            Self::UnauthorizedData(reason) => {
                write!(formatter, "conversation data is unauthorized: {reason}")
            }
            Self::MissingKey => formatter.write_str("conversation key is unavailable"),
            Self::RecipientMismatch => {
                formatter.write_str("conversation envelope names a different recipient key")
            }
            Self::InvalidSignature => {
                formatter.write_str("conversation record signature is invalid")
            }
            Self::AuthenticatedOpen => {
                formatter.write_str("conversation authenticated opening failed")
            }
            Self::LocalSealingFailure => {
                formatter.write_str("conversation data could not be sealed locally")
            }
            Self::RandomnessUnavailable => {
                formatter.write_str("operating-system randomness is unavailable")
            }
        }
    }
}

impl std::error::Error for ConversationError {}
