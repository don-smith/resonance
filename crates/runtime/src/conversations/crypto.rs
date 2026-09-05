//! Fixed-suite message sealing and per-recipient epoch-key delivery.

use std::convert::Infallible;

use chacha20poly1305::{
    aead::{Aead as _, KeyInit as _, Payload},
    XChaCha20Poly1305, XNonce,
};
use hpke::{
    aead::ChaCha20Poly1305,
    kdf::HkdfSha256,
    kem::X25519HkdfSha256,
    rand_core::{TryCryptoRng, TryRng},
    Deserializable as _, Kem as _, OpModeR, OpModeS, Serializable as _,
};
use zeroize::Zeroizing;

use crate::identity::InstallationIdentity;

use super::{
    wire::{
        self, ConversationRecordV1, ExactRecordV1, MembershipHead, MessageRecordV1,
        PublicIdentityBytes, RecordId, WorkspaceId,
    },
    ConversationError,
};

const KEY_BYTES: usize = 32;
const MESSAGE_NONCE_BYTES: usize = 24;
const AEAD_TAG_BYTES: usize = 16;
pub const HPKE_ENCAPSULATION_BYTES: usize = 32;
pub const HPKE_WRAPPED_EPOCH_KEY_BYTES: usize = KEY_BYTES + AEAD_TAG_BYTES;
type Kem = X25519HkdfSha256;
type Kdf = HkdfSha256;
type HpkeAead = ChaCha20Poly1305;

#[derive(Clone)]
pub struct EpochKey(Zeroizing<[u8; KEY_BYTES]>);

impl EpochKey {
    pub fn generate() -> Result<Self, ConversationError> {
        let mut bytes = [0; KEY_BYTES];
        getrandom::fill(&mut bytes).map_err(|_| ConversationError::RandomnessUnavailable)?;
        Ok(Self(Zeroizing::new(bytes)))
    }

    pub(crate) fn from_bytes(bytes: [u8; KEY_BYTES]) -> Self {
        Self(Zeroizing::new(bytes))
    }

    fn expose<R>(&self, use_key: impl FnOnce(&[u8; KEY_BYTES]) -> R) -> R {
        use_key(&self.0)
    }
}

impl PartialEq for EpochKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_ref() == other.0.as_ref()
    }
}

impl Eq for EpochKey {}

impl std::fmt::Debug for EpochKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("EpochKey([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecipientPublicKey([u8; KEY_BYTES]);

impl RecipientPublicKey {
    pub fn from_bytes(bytes: [u8; KEY_BYTES]) -> Result<Self, ConversationError> {
        <Kem as hpke::Kem>::PublicKey::from_bytes(&bytes)
            .map_err(|_| ConversationError::MalformedBytes("recipient public key"))?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; KEY_BYTES] {
        &self.0
    }

    fn backend(&self) -> Result<<Kem as hpke::Kem>::PublicKey, ConversationError> {
        <Kem as hpke::Kem>::PublicKey::from_bytes(&self.0)
            .map_err(|_| ConversationError::MalformedBytes("recipient public key"))
    }
}

#[derive(Clone)]
pub struct RecipientPrivateKey(Zeroizing<[u8; KEY_BYTES]>);

impl RecipientPrivateKey {
    pub fn generate() -> Result<(Self, RecipientPublicKey), ConversationError> {
        let mut ikm = [0; KEY_BYTES];
        getrandom::fill(&mut ikm).map_err(|_| ConversationError::RandomnessUnavailable)?;
        Ok(Self::from_ikm(ikm))
    }

    pub(crate) fn from_ikm(ikm: [u8; KEY_BYTES]) -> (Self, RecipientPublicKey) {
        let ikm = Zeroizing::new(ikm);
        let (private, public) = Kem::derive_keypair(ikm.as_ref());
        let private_bytes: [u8; KEY_BYTES] = private.to_bytes().into();
        let public_bytes: [u8; KEY_BYTES] = public.to_bytes().into();
        (
            Self(Zeroizing::new(private_bytes)),
            RecipientPublicKey(public_bytes),
        )
    }

    pub fn public_key(&self) -> Result<RecipientPublicKey, ConversationError> {
        let private = self.backend()?;
        let public = Kem::sk_to_pk(&private);
        Ok(RecipientPublicKey(public.to_bytes().into()))
    }

    fn backend(&self) -> Result<<Kem as hpke::Kem>::PrivateKey, ConversationError> {
        <Kem as hpke::Kem>::PrivateKey::from_bytes(self.0.as_ref())
            .map_err(|_| ConversationError::MalformedBytes("recipient private key"))
    }
}

impl std::fmt::Debug for RecipientPrivateKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("RecipientPrivateKey([REDACTED])")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageHeaderInputV1 {
    pub workspace_id: WorkspaceId,
    pub channel_id: [u8; 16],
    pub authorization_epoch: MembershipHead,
    pub channel_head: RecordId,
    pub author_sequence: u64,
    pub lamport: u64,
    pub created_at: i64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochEnvelopeContextV1 {
    pub workspace_id: WorkspaceId,
    pub previous_membership_head: Option<MembershipHead>,
    pub resulting_membership_head: MembershipHead,
    pub coordinator: PublicIdentityBytes,
    pub recipient: PublicIdentityBytes,
    pub recipient_public_key: RecipientPublicKey,
    pub recipient_key_record_id: RecordId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HpkeEpochEnvelopeV1 {
    pub encapsulation: [u8; HPKE_ENCAPSULATION_BYTES],
    pub ciphertext: [u8; HPKE_WRAPPED_EPOCH_KEY_BYTES],
}

pub fn seal_message(
    identity: &InstallationIdentity,
    key: &EpochKey,
    header: MessageHeaderInputV1,
    markdown: &str,
) -> Result<ExactRecordV1, ConversationError> {
    let mut nonce = [0; MESSAGE_NONCE_BYTES];
    getrandom::fill(&mut nonce).map_err(|_| ConversationError::RandomnessUnavailable)?;
    seal_message_with_nonce(identity, key, header, markdown, nonce)
}

pub(crate) fn seal_message_with_nonce(
    identity: &InstallationIdentity,
    key: &EpochKey,
    header: MessageHeaderInputV1,
    markdown: &str,
    nonce: [u8; MESSAGE_NONCE_BYTES],
) -> Result<ExactRecordV1, ConversationError> {
    if markdown.len() > wire::MAX_MARKDOWN_BYTES {
        return Err(ConversationError::SizeLimit {
            field: "message Markdown",
            limit: wire::MAX_MARKDOWN_BYTES,
        });
    }
    let mut record = MessageRecordV1 {
        workspace_id: header.workspace_id,
        channel_id: header.channel_id,
        authorization_epoch: header.authorization_epoch,
        channel_head: header.channel_head,
        author: *identity.public_identity().as_bytes(),
        author_sequence: header.author_sequence,
        lamport: header.lamport,
        created_at: header.created_at,
        encryption_suite: wire::MESSAGE_ENCRYPTION_SUITE_V1,
        body_format: wire::MESSAGE_BODY_FORMAT_MARKDOWN_V1,
        nonce,
        ciphertext: vec![0; AEAD_TAG_BYTES],
    };
    let associated_data = wire::message_header_bytes(&record);
    let body = encode_message_body(markdown);
    record.ciphertext = key.expose(|bytes| {
        let cipher = XChaCha20Poly1305::new_from_slice(bytes)
            .map_err(|_| ConversationError::LocalSealingFailure)?;
        cipher
            .encrypt(
                &XNonce::from(nonce),
                Payload {
                    msg: &body,
                    aad: &associated_data,
                },
            )
            .map_err(|_| ConversationError::LocalSealingFailure)
    })?;
    ExactRecordV1::author(ConversationRecordV1::Message(record), identity)
}

pub fn open_message(record: &MessageRecordV1, key: &EpochKey) -> Result<String, ConversationError> {
    if record.encryption_suite != wire::MESSAGE_ENCRYPTION_SUITE_V1 {
        return Err(ConversationError::UnsupportedSuite(record.encryption_suite));
    }
    if record.body_format != wire::MESSAGE_BODY_FORMAT_MARKDOWN_V1 {
        return Err(ConversationError::UnsupportedVersion {
            family: wire::MESSAGE_FAMILY,
            version: record.body_format,
        });
    }
    let associated_data = wire::message_header_bytes(record);
    let plaintext = key.expose(|bytes| {
        let cipher = XChaCha20Poly1305::new_from_slice(bytes)
            .map_err(|_| ConversationError::AuthenticatedOpen)?;
        cipher
            .decrypt(
                &XNonce::from(record.nonce),
                Payload {
                    msg: &record.ciphertext,
                    aad: &associated_data,
                },
            )
            .map_err(|_| ConversationError::AuthenticatedOpen)
    })?;
    decode_message_body(&plaintext)
}

pub fn wrap_epoch_key(
    key: &EpochKey,
    context: &EpochEnvelopeContextV1,
) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
    let mut randomness = [0; KEY_BYTES];
    getrandom::fill(&mut randomness).map_err(|_| ConversationError::RandomnessUnavailable)?;
    wrap_epoch_key_with_randomness(key, context, randomness)
}

pub(crate) fn wrap_epoch_key_with_randomness(
    key: &EpochKey,
    context: &EpochEnvelopeContextV1,
    randomness: [u8; KEY_BYTES],
) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
    let public = context.recipient_public_key.backend()?;
    let info = epoch_envelope_context_bytes(context);
    let mut rng = OneShotRandom::new(randomness);
    let (encapsulation, ciphertext) = key.expose(|bytes| {
        hpke::single_shot_seal_with_rng::<HpkeAead, Kdf, Kem>(
            &OpModeS::Base,
            &public,
            &info,
            bytes,
            &[],
            &mut rng,
        )
        .map_err(|_| ConversationError::LocalSealingFailure)
    })?;
    let encapsulation = encapsulation.to_bytes().into();
    let ciphertext = ciphertext
        .try_into()
        .map_err(|_| ConversationError::LocalSealingFailure)?;
    Ok(HpkeEpochEnvelopeV1 {
        encapsulation,
        ciphertext,
    })
}

pub fn unwrap_epoch_key(
    envelope: &HpkeEpochEnvelopeV1,
    recipient: &RecipientPrivateKey,
    context: &EpochEnvelopeContextV1,
) -> Result<EpochKey, ConversationError> {
    if recipient.public_key()? != context.recipient_public_key {
        return Err(ConversationError::RecipientMismatch);
    }
    let private = recipient.backend()?;
    let encapsulation = <Kem as hpke::Kem>::EncappedKey::from_bytes(&envelope.encapsulation)
        .map_err(|_| ConversationError::MalformedBytes("HPKE encapsulation"))?;
    let info = epoch_envelope_context_bytes(context);
    let mut plaintext = hpke::single_shot_open::<HpkeAead, Kdf, Kem>(
        &OpModeR::Base,
        &private,
        &encapsulation,
        &info,
        &envelope.ciphertext,
        &[],
    )
    .map_err(|_| ConversationError::AuthenticatedOpen)?;
    let key: [u8; KEY_BYTES] = plaintext
        .as_slice()
        .try_into()
        .map_err(|_| ConversationError::AuthenticatedOpen)?;
    plaintext.fill(0);
    Ok(EpochKey::from_bytes(key))
}

pub trait MessageAuthority {
    fn permits(&self, record: &MessageRecordV1) -> bool;
}

pub trait EpochKeyLookup {
    fn find(&self, epoch: &MembershipHead) -> Result<EpochKey, ConversationError>;
}

pub trait AuthenticatedMessageOpener {
    fn open(&self, record: &MessageRecordV1, key: &EpochKey) -> Result<String, ConversationError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct XChaChaMessageOpener;

impl AuthenticatedMessageOpener for XChaChaMessageOpener {
    fn open(&self, record: &MessageRecordV1, key: &EpochKey) -> Result<String, ConversationError> {
        open_message(record, key)
    }
}

pub fn decode_validate_and_open_message(
    bytes: &[u8],
    expected_workspace: &WorkspaceId,
    authority: &impl MessageAuthority,
    keys: &impl EpochKeyLookup,
    opener: &impl AuthenticatedMessageOpener,
) -> Result<String, ConversationError> {
    let exact = ExactRecordV1::decode(bytes)?;
    let ConversationRecordV1::Message(record) = exact.record() else {
        return Err(ConversationError::UnauthorizedData(
            "record is not a message",
        ));
    };
    if &record.workspace_id != expected_workspace {
        return Err(ConversationError::UnauthorizedData("wrong workspace"));
    }
    if !authority.permits(record) {
        return Err(ConversationError::UnauthorizedData(
            "membership or channel authority",
        ));
    }
    let key = keys.find(&record.authorization_epoch)?;
    opener.open(record, &key)
}

fn encode_message_body(markdown: &str) -> Vec<u8> {
    let mut output = vec![wire::MESSAGE_BODY_FORMAT_MARKDOWN_V1];
    wire::put_bytes(&mut output, markdown.as_bytes());
    output
}

fn decode_message_body(bytes: &[u8]) -> Result<String, ConversationError> {
    let Some((&format, remaining)) = bytes.split_first() else {
        return Err(ConversationError::MalformedBytes(
            "missing message body format",
        ));
    };
    if format != wire::MESSAGE_BODY_FORMAT_MARKDOWN_V1 {
        return Err(ConversationError::UnsupportedVersion {
            family: wire::MESSAGE_FAMILY,
            version: format,
        });
    }
    let (length, length_bytes) = decode_u64_prefix(remaining)?;
    let length = usize::try_from(length)
        .map_err(|_| ConversationError::MalformedBytes("message body length"))?;
    if length > wire::MAX_MARKDOWN_BYTES {
        return Err(ConversationError::SizeLimit {
            field: "message Markdown",
            limit: wire::MAX_MARKDOWN_BYTES,
        });
    }
    let markdown = remaining
        .get(length_bytes..)
        .ok_or(ConversationError::MalformedBytes("message body extent"))?;
    if markdown.len() != length {
        return Err(ConversationError::MalformedBytes(
            "message body trailing or truncated bytes",
        ));
    }
    std::str::from_utf8(markdown)
        .map(str::to_owned)
        .map_err(|_| ConversationError::MalformedBytes("message Markdown UTF-8"))
}

fn decode_u64_prefix(bytes: &[u8]) -> Result<(u64, usize), ConversationError> {
    use commonware_codec::Encode as _;

    let mut decoder = commonware_codec::varint::Decoder::<u64>::new();
    for (index, byte) in bytes.iter().copied().take(10).enumerate() {
        if let Some(value) = decoder
            .feed(byte)
            .map_err(|_| ConversationError::MalformedBytes("message body integer"))?
        {
            let encoded = commonware_codec::varint::UInt(value).encode();
            if encoded.as_ref() != &bytes[..=index] {
                return Err(ConversationError::MalformedBytes(
                    "non-minimal message body integer",
                ));
            }
            return Ok((value, index + 1));
        }
    }
    Err(ConversationError::MalformedBytes(
        "truncated message body integer",
    ))
}

fn epoch_envelope_context_bytes(context: &EpochEnvelopeContextV1) -> Vec<u8> {
    let mut output = Vec::new();
    wire::put_bytes(
        &mut output,
        b"resonance.conversation-epoch-envelope-context.v1",
    );
    output.push(wire::FORMAT_VERSION_V1);
    output.push(wire::EPOCH_ENVELOPE_SUITE_V1);
    output.extend_from_slice(&context.workspace_id);
    match context.previous_membership_head {
        None => output.push(0),
        Some(previous) => {
            output.push(1);
            output.extend_from_slice(&previous);
        }
    }
    output.extend_from_slice(&context.resulting_membership_head);
    output.extend_from_slice(&context.coordinator);
    output.extend_from_slice(&context.recipient);
    output.extend_from_slice(context.recipient_public_key.as_bytes());
    output.extend_from_slice(&context.recipient_key_record_id);
    output
}

struct OneShotRandom {
    bytes: Zeroizing<[u8; KEY_BYTES]>,
    position: usize,
}

impl OneShotRandom {
    fn new(bytes: [u8; KEY_BYTES]) -> Self {
        Self {
            bytes: Zeroizing::new(bytes),
            position: 0,
        }
    }

    fn take(&mut self, destination: &mut [u8]) {
        let end = self
            .position
            .checked_add(destination.len())
            .expect("pinned HPKE random request length cannot overflow");
        let source = self
            .bytes
            .get(self.position..end)
            .expect("pinned X25519 HPKE consumes exactly 32 random bytes");
        destination.copy_from_slice(source);
        self.position = end;
    }
}

impl TryRng for OneShotRandom {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        let mut bytes = [0; 4];
        self.take(&mut bytes);
        Ok(u32::from_le_bytes(bytes))
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let mut bytes = [0; 8];
        self.take(&mut bytes);
        Ok(u64::from_le_bytes(bytes))
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        self.take(destination);
        Ok(())
    }
}

impl TryCryptoRng for OneShotRandom {}
