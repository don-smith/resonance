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
    mesh::{ConversationMeshError, ProductionConversationMesh},
    runtime::{ConversationRuntime, ConversationRuntimeError},
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

pub fn seal_runtime_message(
    runtime: &ConversationRuntime,
    channel_id: super::wire::ChannelId,
    markdown: &str,
    author_sequence: u64,
    lamport: u64,
    created_at: i64,
    nonce: [u8; 24],
) -> Result<ExactRecordV1, ConversationRuntimeError> {
    runtime.seal_fixture_message(
        channel_id,
        markdown,
        author_sequence,
        lamport,
        created_at,
        nonce,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn seal_runtime_message_for_head(
    runtime: &ConversationRuntime,
    channel_id: super::wire::ChannelId,
    channel_head: super::wire::RecordId,
    markdown: &str,
    author_sequence: u64,
    lamport: u64,
    created_at: i64,
    nonce: [u8; 24],
) -> Result<ExactRecordV1, ConversationRuntimeError> {
    runtime.seal_fixture_message_for_head(
        channel_id,
        channel_head,
        markdown,
        author_sequence,
        lamport,
        created_at,
        nonce,
    )
}

pub fn outbox_exact_records(
    runtime: &ConversationRuntime,
) -> Result<Vec<Vec<u8>>, ConversationRuntimeError> {
    runtime.outbox_exact_records()
}

pub fn records_eligible_for(
    runtime: &ConversationRuntime,
    requester: crate::identity::PublicIdentity,
) -> Result<Vec<Vec<u8>>, ConversationRuntimeError> {
    runtime.records_eligible_for(requester)
}

pub fn force_mesh_thread_panic(
    mesh: &ProductionConversationMesh,
) -> Result<(), ConversationMeshError> {
    mesh.force_thread_panic()
}

pub fn wrap_epoch_key(
    key: &EpochKey,
    context: &EpochEnvelopeContextV1,
    hpke_randomness: [u8; 32],
) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
    wrap_epoch_key_with_randomness(key, context, hpke_randomness)
}
