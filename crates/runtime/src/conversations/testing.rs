//! Explicit deterministic adapters for compatibility fixtures.
//!
//! Production conversation adapters use the operating-system-random methods in
//! [`super::crypto`]. These entry points require fixture bytes from the caller.

use crate::identity::InstallationIdentity;

use super::{
    crypto::{
        seal_message_with_nonce, wrap_epoch_key_with_randomness, EpochEnvelopeContextV1, EpochKey,
        HpkeEpochEnvelopeV1, MessageHeaderInputV1, RecipientPrivateKey, RecipientPublicKey,
    },
    wire::ExactRecordV1,
    ConversationError,
};

#[must_use]
pub fn epoch_key(bytes: [u8; 32]) -> EpochKey {
    EpochKey::from_bytes(bytes)
}

#[must_use]
pub fn recipient_key_pair(
    input_key_material: [u8; 32],
) -> (RecipientPrivateKey, RecipientPublicKey) {
    RecipientPrivateKey::from_ikm(input_key_material)
}

pub fn seal_message(
    identity: &InstallationIdentity,
    key: &EpochKey,
    header: MessageHeaderInputV1,
    markdown: &str,
    nonce: [u8; 24],
) -> Result<ExactRecordV1, ConversationError> {
    seal_message_with_nonce(identity, key, header, markdown, nonce)
}

pub fn wrap_epoch_key(
    key: &EpochKey,
    context: &EpochEnvelopeContextV1,
    hpke_randomness: [u8; 32],
) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
    wrap_epoch_key_with_randomness(key, context, hpke_randomness)
}
