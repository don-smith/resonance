//! Resonance-owned canonical v1 conversation record families.

use commonware_codec::{varint::UInt, Encode as _};
use commonware_cryptography::{ed25519, Signer as _, Verifier as _};

use crate::identity::InstallationIdentity;

use super::ConversationError;

pub const FAMILY_MARKER: &[u8; 4] = b"RSCV";
pub const FORMAT_VERSION_V1: u8 = 1;
pub const MESSAGE_ENCRYPTION_SUITE_V1: u8 = 1;
pub const EPOCH_ENVELOPE_SUITE_V1: u8 = 1;
pub const MESSAGE_BODY_FORMAT_MARKDOWN_V1: u8 = 1;

pub const CHANNEL_FAMILY: u8 = 1;
pub const MESSAGE_FAMILY: u8 = 2;
pub const RECIPIENT_KEY_FAMILY: u8 = 3;
pub const EPOCH_FAMILY: u8 = 4;
pub const ADDRESS_NOTICE_FAMILY: u8 = 5;
pub const ACKNOWLEDGEMENT_FAMILY: u8 = 6;
pub const RECOVERY_REQUEST_FAMILY: u8 = 7;
pub const RECOVERY_RESPONSE_FAMILY: u8 = 8;

pub const MAX_MARKDOWN_BYTES: usize = 16_384;
pub const MAX_CHANNEL_NAME_BYTES: usize = 80;
pub const MAX_MEMBERS_PER_EPOCH: usize = 128;
pub const MAX_RECIPIENT_ENTRIES: usize = MAX_MEMBERS_PER_EPOCH;
pub const MAX_RECORD_BYTES: usize = 131_072;
pub const MAX_FRAME_BYTES: usize = 131_200;
pub const MAX_ADDRESSES_PER_NOTICE: usize = 16;
pub const MAX_ADDRESS_BYTES: usize = 256;
pub const MAX_ACKNOWLEDGEMENT_ITEMS: usize = 256;
pub const MAX_RECOVERY_HEADS: usize = 128;
pub const MAX_RECOVERY_RANGES: usize = 256;
pub const MAX_RECOVERY_RESPONSE_RECORDS: usize = 128;
pub const MAX_RECOVERY_RECORD_BYTES: usize = 65_536;
pub const MAX_MESSAGE_CIPHERTEXT_BYTES: usize = MAX_MARKDOWN_BYTES + 32;

pub type WorkspaceId = [u8; 32];
pub type RecordId = [u8; 32];
pub type MembershipHead = [u8; 32];
pub type PublicIdentityBytes = [u8; 32];
pub type ChannelId = [u8; 16];

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChannelOperationV1 {
    Create { name: String },
    Rename { name: String },
    Archive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelRecordV1 {
    pub workspace_id: WorkspaceId,
    pub channel_id: ChannelId,
    pub authorization_epoch: MembershipHead,
    pub creator: PublicIdentityBytes,
    pub author: PublicIdentityBytes,
    pub author_sequence: u64,
    pub created_at: i64,
    pub predecessor: Option<RecordId>,
    pub operation: ChannelOperationV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageRecordV1 {
    pub workspace_id: WorkspaceId,
    pub channel_id: ChannelId,
    pub authorization_epoch: MembershipHead,
    pub channel_head: RecordId,
    pub author: PublicIdentityBytes,
    pub author_sequence: u64,
    pub lamport: u64,
    pub created_at: i64,
    pub encryption_suite: u8,
    pub body_format: u8,
    pub nonce: [u8; 24],
    pub ciphertext: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientKeyRecordV1 {
    pub workspace_id: WorkspaceId,
    pub installation: PublicIdentityBytes,
    pub suite: u8,
    pub generation: u64,
    pub recipient_public_key: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochRecipientV1 {
    pub member: PublicIdentityBytes,
    pub recipient_key_record_id: RecordId,
    pub encapsulation: [u8; 32],
    pub wrapped_epoch_key: [u8; 48],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochRecordV1 {
    pub workspace_id: WorkspaceId,
    pub previous_membership_head: Option<MembershipHead>,
    pub resulting_membership_head: MembershipHead,
    pub coordinator: PublicIdentityBytes,
    pub suite: u8,
    pub recipients: Vec<EpochRecipientV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressNoticeV1 {
    pub workspace_id: WorkspaceId,
    pub sender: PublicIdentityBytes,
    pub observed_membership_head: MembershipHead,
    pub generation: u64,
    pub expires_at: i64,
    pub addresses: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcknowledgementV1 {
    pub workspace_id: WorkspaceId,
    pub sender: PublicIdentityBytes,
    pub record_ids: Vec<RecordId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryHeadV1 {
    pub author: PublicIdentityBytes,
    pub highest_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecoveryRangeV1 {
    pub author: PublicIdentityBytes,
    pub first_sequence: u64,
    pub last_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryRequestV1 {
    pub workspace_id: WorkspaceId,
    pub sender: PublicIdentityBytes,
    pub heads: Vec<RecoveryHeadV1>,
    pub missing_ranges: Vec<RecoveryRangeV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryResponseV1 {
    pub workspace_id: WorkspaceId,
    pub sender: PublicIdentityBytes,
    pub records: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationRecordV1 {
    Channel(ChannelRecordV1),
    Message(MessageRecordV1),
    RecipientKey(RecipientKeyRecordV1),
    Epoch(EpochRecordV1),
    AddressNotice(AddressNoticeV1),
    Acknowledgement(AcknowledgementV1),
    RecoveryRequest(RecoveryRequestV1),
    RecoveryResponse(RecoveryResponseV1),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExactRecordV1 {
    record: ConversationRecordV1,
    signature: [u8; 64],
    bytes: Vec<u8>,
    id: RecordId,
}

impl ExactRecordV1 {
    pub fn author(
        record: ConversationRecordV1,
        identity: &InstallationIdentity,
    ) -> Result<Self, ConversationError> {
        record.validate()?;
        if record.signer() != *identity.public_identity().as_bytes() {
            return Err(ConversationError::UnauthorizedData(
                "record signer does not match installation identity",
            ));
        }
        let unsigned = encode_unsigned(&record)?;
        let signer = identity
            .commonware_signer()
            .map_err(|_| ConversationError::LocalSealingFailure)?;
        let signature = signer.sign(record.signature_domain(), &unsigned);
        let signature: [u8; 64] = signature
            .as_ref()
            .try_into()
            .map_err(|_| ConversationError::LocalSealingFailure)?;
        finish_exact(record, signature, unsigned)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ConversationError> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(ConversationError::SizeLimit {
                field: "record bytes",
                limit: MAX_RECORD_BYTES,
            });
        }
        let mut reader = Reader::new(bytes);
        if reader.array::<4>()? != *FAMILY_MARKER {
            return Err(ConversationError::MalformedBytes("family marker"));
        }
        let family = reader.byte()?;
        if !(CHANNEL_FAMILY..=RECOVERY_RESPONSE_FAMILY).contains(&family) {
            return Err(ConversationError::UnsupportedFamily(family));
        }
        let version = reader.byte()?;
        if version != FORMAT_VERSION_V1 {
            return Err(ConversationError::UnsupportedVersion { family, version });
        }
        let payload_length = reader.length()?;
        let payload = reader.exact(payload_length)?;
        let signature = reader.array::<64>()?;
        reader.finish()?;

        let record = decode_payload(family, payload)?;
        record.validate()?;
        let unsigned_length = bytes
            .len()
            .checked_sub(64)
            .ok_or(ConversationError::MalformedBytes("signature extent"))?;
        let unsigned = &bytes[..unsigned_length];
        let expected_unsigned = encode_unsigned(&record)?;
        if expected_unsigned != unsigned {
            return Err(ConversationError::MalformedBytes(
                "record is not canonically encoded",
            ));
        }
        verify_signature(&record, unsigned, &signature)?;
        finish_exact(record, signature, unsigned.to_vec())
    }

    #[must_use]
    pub fn record(&self) -> &ConversationRecordV1 {
        &self.record
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn id(&self) -> &RecordId {
        &self.id
    }

    #[must_use]
    pub const fn signature(&self) -> &[u8; 64] {
        &self.signature
    }
}

impl ConversationRecordV1 {
    #[must_use]
    pub const fn family(&self) -> u8 {
        match self {
            Self::Channel(_) => CHANNEL_FAMILY,
            Self::Message(_) => MESSAGE_FAMILY,
            Self::RecipientKey(_) => RECIPIENT_KEY_FAMILY,
            Self::Epoch(_) => EPOCH_FAMILY,
            Self::AddressNotice(_) => ADDRESS_NOTICE_FAMILY,
            Self::Acknowledgement(_) => ACKNOWLEDGEMENT_FAMILY,
            Self::RecoveryRequest(_) => RECOVERY_REQUEST_FAMILY,
            Self::RecoveryResponse(_) => RECOVERY_RESPONSE_FAMILY,
        }
    }

    #[must_use]
    pub const fn signer(&self) -> PublicIdentityBytes {
        match self {
            Self::Channel(record) => record.author,
            Self::Message(record) => record.author,
            Self::RecipientKey(record) => record.installation,
            Self::Epoch(record) => record.coordinator,
            Self::AddressNotice(record) => record.sender,
            Self::Acknowledgement(record) => record.sender,
            Self::RecoveryRequest(record) => record.sender,
            Self::RecoveryResponse(record) => record.sender,
        }
    }

    const fn signature_domain(&self) -> &'static [u8] {
        match self {
            Self::Channel(_) => b"resonance.conversation-channel.v1",
            Self::Message(_) => b"resonance.conversation-message.v1",
            Self::RecipientKey(_) => b"resonance.conversation-recipient-key.v1",
            Self::Epoch(_) => b"resonance.conversation-epoch.v1",
            Self::AddressNotice(_) => b"resonance.conversation-address-notice.v1",
            Self::Acknowledgement(_) => b"resonance.conversation-acknowledgement.v1",
            Self::RecoveryRequest(_) => b"resonance.conversation-recovery-request.v1",
            Self::RecoveryResponse(_) => b"resonance.conversation-recovery-response.v1",
        }
    }

    fn validate(&self) -> Result<(), ConversationError> {
        match self {
            Self::Channel(record) => {
                let name = match &record.operation {
                    ChannelOperationV1::Create { name } | ChannelOperationV1::Rename { name } => {
                        Some(name)
                    }
                    ChannelOperationV1::Archive => None,
                };
                if let Some(name) = name {
                    check_utf8_text(name, "channel name", MAX_CHANNEL_NAME_BYTES, false)?;
                }
                match (&record.operation, record.predecessor) {
                    (ChannelOperationV1::Create { .. }, None) => {}
                    (ChannelOperationV1::Rename { .. } | ChannelOperationV1::Archive, Some(_)) => {}
                    _ => {
                        return Err(ConversationError::MalformedBytes(
                            "channel predecessor does not match operation",
                        ));
                    }
                }
            }
            Self::Message(record) => {
                if record.encryption_suite != MESSAGE_ENCRYPTION_SUITE_V1 {
                    return Err(ConversationError::UnsupportedSuite(record.encryption_suite));
                }
                if record.body_format != MESSAGE_BODY_FORMAT_MARKDOWN_V1 {
                    return Err(ConversationError::UnsupportedVersion {
                        family: MESSAGE_FAMILY,
                        version: record.body_format,
                    });
                }
                check_len(
                    record.ciphertext.len(),
                    "message ciphertext",
                    MAX_MESSAGE_CIPHERTEXT_BYTES,
                )?;
                if record.ciphertext.len() < 16 {
                    return Err(ConversationError::MalformedBytes(
                        "message ciphertext is shorter than its authentication tag",
                    ));
                }
            }
            Self::RecipientKey(record) => {
                if record.suite != EPOCH_ENVELOPE_SUITE_V1 {
                    return Err(ConversationError::UnsupportedSuite(record.suite));
                }
            }
            Self::Epoch(record) => {
                if record.suite != EPOCH_ENVELOPE_SUITE_V1 {
                    return Err(ConversationError::UnsupportedSuite(record.suite));
                }
                check_len(
                    record.recipients.len(),
                    "epoch recipients",
                    MAX_RECIPIENT_ENTRIES,
                )?;
                if record.recipients.is_empty() {
                    return Err(ConversationError::MalformedBytes(
                        "epoch recipient set is empty",
                    ));
                }
                ensure_strictly_sorted_by(
                    &record.recipients,
                    |recipient| recipient.member,
                    "epoch recipients",
                )?;
            }
            Self::AddressNotice(record) => {
                check_len(
                    record.addresses.len(),
                    "notice addresses",
                    MAX_ADDRESSES_PER_NOTICE,
                )?;
                if record.addresses.is_empty() {
                    return Err(ConversationError::MalformedBytes("address notice is empty"));
                }
                for address in &record.addresses {
                    check_utf8_text(address, "address", MAX_ADDRESS_BYTES, false)?;
                }
                ensure_strictly_sorted_by(
                    &record.addresses,
                    |address| address.as_bytes().to_vec(),
                    "notice addresses",
                )?;
            }
            Self::Acknowledgement(record) => {
                check_len(
                    record.record_ids.len(),
                    "acknowledgement items",
                    MAX_ACKNOWLEDGEMENT_ITEMS,
                )?;
                ensure_strictly_sorted_by(
                    &record.record_ids,
                    |record_id| *record_id,
                    "acknowledgement items",
                )?;
            }
            Self::RecoveryRequest(record) => {
                check_len(record.heads.len(), "recovery heads", MAX_RECOVERY_HEADS)?;
                check_len(
                    record.missing_ranges.len(),
                    "recovery ranges",
                    MAX_RECOVERY_RANGES,
                )?;
                ensure_strictly_sorted_by(&record.heads, |head| head.author, "recovery heads")?;
                ensure_strictly_sorted_by(
                    &record.missing_ranges,
                    |range| (range.author, range.first_sequence, range.last_sequence),
                    "recovery ranges",
                )?;
                if record
                    .missing_ranges
                    .iter()
                    .any(|range| range.first_sequence > range.last_sequence)
                {
                    return Err(ConversationError::MalformedBytes(
                        "recovery range is reversed",
                    ));
                }
            }
            Self::RecoveryResponse(record) => {
                check_len(
                    record.records.len(),
                    "recovery response records",
                    MAX_RECOVERY_RESPONSE_RECORDS,
                )?;
                for exact in &record.records {
                    check_len(
                        exact.len(),
                        "recovery record bytes",
                        MAX_RECOVERY_RECORD_BYTES,
                    )?;
                    if exact.is_empty() {
                        return Err(ConversationError::MalformedBytes("empty recovery record"));
                    }
                }
                ensure_strictly_sorted_by(
                    &record.records,
                    |exact| blake3::hash(exact).as_bytes().to_owned(),
                    "recovery response records",
                )?;
            }
        }
        Ok(())
    }
}

fn finish_exact(
    record: ConversationRecordV1,
    signature: [u8; 64],
    mut unsigned: Vec<u8>,
) -> Result<ExactRecordV1, ConversationError> {
    unsigned.extend_from_slice(&signature);
    if unsigned.len() > MAX_RECORD_BYTES {
        return Err(ConversationError::SizeLimit {
            field: "record bytes",
            limit: MAX_RECORD_BYTES,
        });
    }
    let id = *blake3::hash(&unsigned).as_bytes();
    Ok(ExactRecordV1 {
        record,
        signature,
        bytes: unsigned,
        id,
    })
}

fn encode_unsigned(record: &ConversationRecordV1) -> Result<Vec<u8>, ConversationError> {
    record.validate()?;
    let payload = encode_payload(record);
    let mut output = Vec::with_capacity(FAMILY_MARKER.len() + payload.len() + 72);
    output.extend_from_slice(FAMILY_MARKER);
    output.push(record.family());
    output.push(FORMAT_VERSION_V1);
    put_u64(&mut output, payload.len() as u64);
    output.extend_from_slice(&payload);
    if output.len() + 64 > MAX_RECORD_BYTES {
        return Err(ConversationError::SizeLimit {
            field: "record bytes",
            limit: MAX_RECORD_BYTES,
        });
    }
    Ok(output)
}

fn verify_signature(
    record: &ConversationRecordV1,
    unsigned: &[u8],
    signature: &[u8; 64],
) -> Result<(), ConversationError> {
    use commonware_codec::DecodeExt as _;

    let public = ed25519::PublicKey::decode(record.signer().as_slice())
        .map_err(|_| ConversationError::InvalidSignature)?;
    let signature = ed25519::Signature::decode(signature.as_slice())
        .map_err(|_| ConversationError::InvalidSignature)?;
    if public.verify(record.signature_domain(), unsigned, &signature) {
        Ok(())
    } else {
        Err(ConversationError::InvalidSignature)
    }
}

fn encode_payload(record: &ConversationRecordV1) -> Vec<u8> {
    let mut output = Vec::new();
    match record {
        ConversationRecordV1::Channel(record) => {
            put_array(&mut output, &record.workspace_id);
            put_array(&mut output, &record.channel_id);
            put_array(&mut output, &record.authorization_epoch);
            put_array(&mut output, &record.creator);
            put_array(&mut output, &record.author);
            put_u64(&mut output, record.author_sequence);
            put_i64(&mut output, record.created_at);
            put_optional_id(&mut output, record.predecessor.as_ref());
            match &record.operation {
                ChannelOperationV1::Create { name } => {
                    output.push(1);
                    put_bytes(&mut output, name.as_bytes());
                }
                ChannelOperationV1::Rename { name } => {
                    output.push(2);
                    put_bytes(&mut output, name.as_bytes());
                }
                ChannelOperationV1::Archive => output.push(3),
            }
        }
        ConversationRecordV1::Message(record) => {
            put_array(&mut output, &record.workspace_id);
            put_array(&mut output, &record.channel_id);
            put_array(&mut output, &record.authorization_epoch);
            put_array(&mut output, &record.channel_head);
            put_array(&mut output, &record.author);
            put_u64(&mut output, record.author_sequence);
            put_u64(&mut output, record.lamport);
            put_i64(&mut output, record.created_at);
            output.push(record.encryption_suite);
            output.push(record.body_format);
            put_array(&mut output, &record.nonce);
            put_bytes(&mut output, &record.ciphertext);
        }
        ConversationRecordV1::RecipientKey(record) => {
            put_array(&mut output, &record.workspace_id);
            put_array(&mut output, &record.installation);
            output.push(record.suite);
            put_u64(&mut output, record.generation);
            put_array(&mut output, &record.recipient_public_key);
        }
        ConversationRecordV1::Epoch(record) => {
            put_array(&mut output, &record.workspace_id);
            put_optional_id(&mut output, record.previous_membership_head.as_ref());
            put_array(&mut output, &record.resulting_membership_head);
            put_array(&mut output, &record.coordinator);
            output.push(record.suite);
            put_u64(&mut output, record.recipients.len() as u64);
            for recipient in &record.recipients {
                put_array(&mut output, &recipient.member);
                put_array(&mut output, &recipient.recipient_key_record_id);
                put_array(&mut output, &recipient.encapsulation);
                put_array(&mut output, &recipient.wrapped_epoch_key);
            }
        }
        ConversationRecordV1::AddressNotice(record) => {
            put_array(&mut output, &record.workspace_id);
            put_array(&mut output, &record.sender);
            put_array(&mut output, &record.observed_membership_head);
            put_u64(&mut output, record.generation);
            put_i64(&mut output, record.expires_at);
            put_u64(&mut output, record.addresses.len() as u64);
            for address in &record.addresses {
                put_bytes(&mut output, address.as_bytes());
            }
        }
        ConversationRecordV1::Acknowledgement(record) => {
            put_array(&mut output, &record.workspace_id);
            put_array(&mut output, &record.sender);
            put_u64(&mut output, record.record_ids.len() as u64);
            for record_id in &record.record_ids {
                put_array(&mut output, record_id);
            }
        }
        ConversationRecordV1::RecoveryRequest(record) => {
            put_array(&mut output, &record.workspace_id);
            put_array(&mut output, &record.sender);
            put_u64(&mut output, record.heads.len() as u64);
            for head in &record.heads {
                put_array(&mut output, &head.author);
                put_u64(&mut output, head.highest_sequence);
            }
            put_u64(&mut output, record.missing_ranges.len() as u64);
            for range in &record.missing_ranges {
                put_array(&mut output, &range.author);
                put_u64(&mut output, range.first_sequence);
                put_u64(&mut output, range.last_sequence);
            }
        }
        ConversationRecordV1::RecoveryResponse(record) => {
            put_array(&mut output, &record.workspace_id);
            put_array(&mut output, &record.sender);
            put_u64(&mut output, record.records.len() as u64);
            for exact in &record.records {
                put_bytes(&mut output, exact);
            }
        }
    }
    output
}

fn decode_payload(family: u8, payload: &[u8]) -> Result<ConversationRecordV1, ConversationError> {
    let mut reader = Reader::new(payload);
    let record = match family {
        CHANNEL_FAMILY => {
            let workspace_id = reader.array()?;
            let channel_id = reader.array()?;
            let authorization_epoch = reader.array()?;
            let creator = reader.array()?;
            let author = reader.array()?;
            let author_sequence = reader.u64()?;
            let created_at = reader.i64()?;
            let predecessor = reader.optional_id()?;
            let operation = match reader.byte()? {
                1 => ChannelOperationV1::Create {
                    name: reader.text("channel name", MAX_CHANNEL_NAME_BYTES, false)?,
                },
                2 => ChannelOperationV1::Rename {
                    name: reader.text("channel name", MAX_CHANNEL_NAME_BYTES, false)?,
                },
                3 => ChannelOperationV1::Archive,
                _ => return Err(ConversationError::MalformedBytes("channel operation tag")),
            };
            ConversationRecordV1::Channel(ChannelRecordV1 {
                workspace_id,
                channel_id,
                authorization_epoch,
                creator,
                author,
                author_sequence,
                created_at,
                predecessor,
                operation,
            })
        }
        MESSAGE_FAMILY => ConversationRecordV1::Message(MessageRecordV1 {
            workspace_id: reader.array()?,
            channel_id: reader.array()?,
            authorization_epoch: reader.array()?,
            channel_head: reader.array()?,
            author: reader.array()?,
            author_sequence: reader.u64()?,
            lamport: reader.u64()?,
            created_at: reader.i64()?,
            encryption_suite: reader.byte()?,
            body_format: reader.byte()?,
            nonce: reader.array()?,
            ciphertext: reader
                .bytes("message ciphertext", MAX_MESSAGE_CIPHERTEXT_BYTES, false)?
                .to_vec(),
        }),
        RECIPIENT_KEY_FAMILY => ConversationRecordV1::RecipientKey(RecipientKeyRecordV1 {
            workspace_id: reader.array()?,
            installation: reader.array()?,
            suite: reader.byte()?,
            generation: reader.u64()?,
            recipient_public_key: reader.array()?,
        }),
        EPOCH_FAMILY => {
            let workspace_id = reader.array()?;
            let previous_membership_head = reader.optional_id()?;
            let resulting_membership_head = reader.array()?;
            let coordinator = reader.array()?;
            let suite = reader.byte()?;
            let count = reader.count("epoch recipients", MAX_RECIPIENT_ENTRIES)?;
            let mut recipients = Vec::with_capacity(count);
            for _ in 0..count {
                recipients.push(EpochRecipientV1 {
                    member: reader.array()?,
                    recipient_key_record_id: reader.array()?,
                    encapsulation: reader.array()?,
                    wrapped_epoch_key: reader.array()?,
                });
            }
            ConversationRecordV1::Epoch(EpochRecordV1 {
                workspace_id,
                previous_membership_head,
                resulting_membership_head,
                coordinator,
                suite,
                recipients,
            })
        }
        ADDRESS_NOTICE_FAMILY => {
            let workspace_id = reader.array()?;
            let sender = reader.array()?;
            let observed_membership_head = reader.array()?;
            let generation = reader.u64()?;
            let expires_at = reader.i64()?;
            let count = reader.count("notice addresses", MAX_ADDRESSES_PER_NOTICE)?;
            let mut addresses = Vec::with_capacity(count);
            for _ in 0..count {
                addresses.push(reader.text("address", MAX_ADDRESS_BYTES, false)?);
            }
            ConversationRecordV1::AddressNotice(AddressNoticeV1 {
                workspace_id,
                sender,
                observed_membership_head,
                generation,
                expires_at,
                addresses,
            })
        }
        ACKNOWLEDGEMENT_FAMILY => {
            let workspace_id = reader.array()?;
            let sender = reader.array()?;
            let count = reader.count("acknowledgement items", MAX_ACKNOWLEDGEMENT_ITEMS)?;
            let mut record_ids = Vec::with_capacity(count);
            for _ in 0..count {
                record_ids.push(reader.array()?);
            }
            ConversationRecordV1::Acknowledgement(AcknowledgementV1 {
                workspace_id,
                sender,
                record_ids,
            })
        }
        RECOVERY_REQUEST_FAMILY => {
            let workspace_id = reader.array()?;
            let sender = reader.array()?;
            let head_count = reader.count("recovery heads", MAX_RECOVERY_HEADS)?;
            let mut heads = Vec::with_capacity(head_count);
            for _ in 0..head_count {
                heads.push(RecoveryHeadV1 {
                    author: reader.array()?,
                    highest_sequence: reader.u64()?,
                });
            }
            let range_count = reader.count("recovery ranges", MAX_RECOVERY_RANGES)?;
            let mut missing_ranges = Vec::with_capacity(range_count);
            for _ in 0..range_count {
                missing_ranges.push(RecoveryRangeV1 {
                    author: reader.array()?,
                    first_sequence: reader.u64()?,
                    last_sequence: reader.u64()?,
                });
            }
            ConversationRecordV1::RecoveryRequest(RecoveryRequestV1 {
                workspace_id,
                sender,
                heads,
                missing_ranges,
            })
        }
        RECOVERY_RESPONSE_FAMILY => {
            let workspace_id = reader.array()?;
            let sender = reader.array()?;
            let count = reader.count("recovery response records", MAX_RECOVERY_RESPONSE_RECORDS)?;
            let mut records = Vec::with_capacity(count);
            for _ in 0..count {
                records.push(
                    reader
                        .bytes("recovery record bytes", MAX_RECOVERY_RECORD_BYTES, false)?
                        .to_vec(),
                );
            }
            ConversationRecordV1::RecoveryResponse(RecoveryResponseV1 {
                workspace_id,
                sender,
                records,
            })
        }
        _ => return Err(ConversationError::UnsupportedFamily(family)),
    };
    reader.finish()?;
    Ok(record)
}

pub(crate) fn message_header_bytes(record: &MessageRecordV1) -> Vec<u8> {
    let mut output = Vec::new();
    put_bytes(&mut output, b"resonance.conversation-message-header.v1");
    put_array(&mut output, &record.workspace_id);
    put_array(&mut output, &record.channel_id);
    put_array(&mut output, &record.authorization_epoch);
    put_array(&mut output, &record.channel_head);
    put_array(&mut output, &record.author);
    put_u64(&mut output, record.author_sequence);
    put_u64(&mut output, record.lamport);
    put_i64(&mut output, record.created_at);
    output.push(record.encryption_suite);
    output.push(record.body_format);
    output
}

fn check_len(actual: usize, field: &'static str, limit: usize) -> Result<(), ConversationError> {
    if actual > limit {
        Err(ConversationError::SizeLimit { field, limit })
    } else {
        Ok(())
    }
}

fn check_utf8_text(
    value: &str,
    field: &'static str,
    limit: usize,
    allow_empty: bool,
) -> Result<(), ConversationError> {
    check_len(value.len(), field, limit)?;
    if !allow_empty && value.is_empty() {
        Err(ConversationError::MalformedBytes("empty UTF-8 field"))
    } else {
        Ok(())
    }
}

fn ensure_strictly_sorted_by<T, K: Ord>(
    values: &[T],
    key: impl Fn(&T) -> K,
    field: &'static str,
) -> Result<(), ConversationError> {
    if values.windows(2).any(|pair| key(&pair[0]) >= key(&pair[1])) {
        Err(ConversationError::MalformedBytes(field))
    } else {
        Ok(())
    }
}

fn put_array<const N: usize>(output: &mut Vec<u8>, value: &[u8; N]) {
    output.extend_from_slice(value);
}

fn put_i64(output: &mut Vec<u8>, value: i64) {
    output.extend_from_slice(&value.to_be_bytes());
}

fn put_optional_id(output: &mut Vec<u8>, value: Option<&RecordId>) {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            put_array(output, value);
        }
    }
}

pub(crate) fn put_u64(output: &mut Vec<u8>, value: u64) {
    output.extend_from_slice(UInt(value).encode().as_ref());
}

pub(crate) fn put_bytes(output: &mut Vec<u8>, value: &[u8]) {
    put_u64(output, value.len() as u64);
    output.extend_from_slice(value);
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn byte(&mut self) -> Result<u8, ConversationError> {
        let byte = self
            .bytes
            .get(self.position)
            .copied()
            .ok_or(ConversationError::MalformedBytes("truncated input"))?;
        self.position += 1;
        Ok(byte)
    }

    fn exact(&mut self, length: usize) -> Result<&'a [u8], ConversationError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(ConversationError::MalformedBytes("length overflow"))?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(ConversationError::MalformedBytes("truncated extent"))?;
        self.position = end;
        Ok(value)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ConversationError> {
        self.exact(N)?
            .try_into()
            .map_err(|_| ConversationError::MalformedBytes("fixed array"))
    }

    fn u64(&mut self) -> Result<u64, ConversationError> {
        let start = self.position;
        let mut decoder = commonware_codec::varint::Decoder::<u64>::new();
        for _ in 0..10 {
            let byte = self.byte()?;
            if let Some(value) = decoder
                .feed(byte)
                .map_err(|_| ConversationError::MalformedBytes("invalid canonical integer"))?
            {
                let canonical = UInt(value).encode();
                if canonical.as_ref() != &self.bytes[start..self.position] {
                    return Err(ConversationError::MalformedBytes(
                        "non-minimal canonical integer",
                    ));
                }
                return Ok(value);
            }
        }
        Err(ConversationError::MalformedBytes(
            "canonical integer exceeds ten bytes",
        ))
    }

    fn length(&mut self) -> Result<usize, ConversationError> {
        usize::try_from(self.u64()?)
            .map_err(|_| ConversationError::MalformedBytes("length does not fit usize"))
    }

    fn i64(&mut self) -> Result<i64, ConversationError> {
        Ok(i64::from_be_bytes(self.array()?))
    }

    fn bytes(
        &mut self,
        field: &'static str,
        limit: usize,
        allow_empty: bool,
    ) -> Result<&'a [u8], ConversationError> {
        let length = self.length()?;
        check_len(length, field, limit)?;
        if !allow_empty && length == 0 {
            return Err(ConversationError::MalformedBytes("empty byte field"));
        }
        self.exact(length)
    }

    fn text(
        &mut self,
        field: &'static str,
        limit: usize,
        allow_empty: bool,
    ) -> Result<String, ConversationError> {
        let bytes = self.bytes(field, limit, allow_empty)?;
        let text = std::str::from_utf8(bytes)
            .map_err(|_| ConversationError::MalformedBytes("invalid UTF-8"))?;
        Ok(text.to_owned())
    }

    fn count(&mut self, field: &'static str, limit: usize) -> Result<usize, ConversationError> {
        let count = self.length()?;
        check_len(count, field, limit)?;
        Ok(count)
    }

    fn optional_id(&mut self) -> Result<Option<RecordId>, ConversationError> {
        match self.byte()? {
            0 => Ok(None),
            1 => self.array().map(Some),
            _ => Err(ConversationError::MalformedBytes("optional ID tag")),
        }
    }

    fn finish(&self) -> Result<(), ConversationError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(ConversationError::MalformedBytes("trailing bytes"))
        }
    }
}
