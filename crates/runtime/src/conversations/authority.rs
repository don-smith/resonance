//! Membership-derived recipient authority and atomic epoch construction.

use std::collections::BTreeMap;

use crate::{
    identity::{InstallationIdentity, PublicIdentity},
    membership_log::{
        MembershipOperationBody, PreparedMembershipTransition, RemovalAuthorizationV1,
    },
    workspace_store::{AtomicMembershipEpochCommit, WorkspaceStore, WorkspaceStoreError},
};

use super::{
    crypto::{
        self, EpochEnvelopeContextV1, EpochKey, HpkeEpochEnvelopeV1, RecipientPrivateKey,
        RecipientPublicKey,
    },
    key_custody::InstallationRecipientKey,
    wire::{
        ChannelOperationV1, ChannelRecordV1, ConversationRecordV1, EpochRecipientV1, EpochRecordV1,
        ExactRecordV1, MembershipHead, RecipientKeyRecordV1, WorkspaceId, EPOCH_ENVELOPE_SUITE_V1,
    },
    ConversationError,
};

#[derive(Debug)]
pub enum ConversationAuthorityError {
    Conversation(ConversationError),
    Store(WorkspaceStoreError),
    InvalidWorkspace,
    MissingRecipientKey(PublicIdentity),
    UnauthorizedCoordinator,
    InvalidPreparedTransition(&'static str),
}

impl std::fmt::Display for ConversationAuthorityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conversation(error) => {
                write!(formatter, "conversation authority failed: {error}")
            }
            Self::Store(error) => write!(formatter, "conversation storage failed: {error}"),
            Self::InvalidWorkspace => formatter.write_str("conversation workspace ID is invalid"),
            Self::MissingRecipientKey(identity) => {
                write!(formatter, "recipient key is unavailable for {identity}")
            }
            Self::UnauthorizedCoordinator => formatter.write_str(
                "only the resulting member who authored the transition may coordinate its epoch",
            ),
            Self::InvalidPreparedTransition(reason) => {
                write!(
                    formatter,
                    "prepared membership transition is invalid: {reason}"
                )
            }
        }
    }
}

impl std::error::Error for ConversationAuthorityError {}

impl From<ConversationError> for ConversationAuthorityError {
    fn from(error: ConversationError) -> Self {
        Self::Conversation(error)
    }
}

impl From<WorkspaceStoreError> for ConversationAuthorityError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Store(error)
    }
}

pub trait EpochMaterialProvider {
    fn generate_epoch_key(&mut self) -> Result<EpochKey, ConversationError>;
    fn wrap_epoch_key(
        &mut self,
        key: &EpochKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<HpkeEpochEnvelopeV1, ConversationError>;
    fn open_epoch_key(
        &mut self,
        envelope: &HpkeEpochEnvelopeV1,
        recipient: &RecipientPrivateKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<EpochKey, ConversationError>;
}

#[derive(Default)]
pub struct OperatingSystemEpochMaterial;

impl EpochMaterialProvider for OperatingSystemEpochMaterial {
    fn generate_epoch_key(&mut self) -> Result<EpochKey, ConversationError> {
        EpochKey::generate()
    }

    fn wrap_epoch_key(
        &mut self,
        key: &EpochKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<HpkeEpochEnvelopeV1, ConversationError> {
        crypto::wrap_epoch_key(key, context)
    }

    fn open_epoch_key(
        &mut self,
        envelope: &HpkeEpochEnvelopeV1,
        recipient: &RecipientPrivateKey,
        context: &EpochEnvelopeContextV1,
    ) -> Result<EpochKey, ConversationError> {
        crypto::unwrap_epoch_key(envelope, recipient, context)
    }
}

#[derive(Clone, Debug)]
pub struct CommittedMembershipEpoch {
    pub epoch: ExactRecordV1,
    pub genesis_channel: Option<ExactRecordV1>,
    pub lookup_peer_set_version: u64,
}

pub struct ConversationAuthority {
    identity: InstallationIdentity,
    workspace_id: WorkspaceId,
    store: WorkspaceStore,
    local_recipient: InstallationRecipientKey,
    recipient_records: BTreeMap<PublicIdentity, ExactRecordV1>,
}

impl ConversationAuthority {
    pub fn open(
        identity: InstallationIdentity,
        workspace_id: &str,
        store: WorkspaceStore,
    ) -> Result<Self, ConversationAuthorityError> {
        let workspace_id = decode_hex_32(workspace_id).ok_or(Self::invalid_workspace())?;
        let local_recipient = InstallationRecipientKey::load(&identity);
        let own_record = local_recipient.author_record(&identity, workspace_id)?;
        store.record_recipient_key(&own_record, false)?;
        let mut authority = Self {
            identity,
            workspace_id,
            store,
            local_recipient,
            recipient_records: BTreeMap::new(),
        };
        for bytes in authority.store.recipient_key_records()? {
            let exact = ExactRecordV1::decode(&bytes)?;
            if let ConversationRecordV1::RecipientKey(record) = exact.record() {
                authority
                    .recipient_records
                    .insert(PublicIdentity::from_bytes(record.installation), exact);
            }
        }
        authority
            .recipient_records
            .insert(authority.identity.public_identity(), own_record);
        Ok(authority)
    }

    const fn invalid_workspace() -> ConversationAuthorityError {
        ConversationAuthorityError::InvalidWorkspace
    }

    pub fn local_recipient_record(&self) -> &ExactRecordV1 {
        self.recipient_records
            .get(&self.identity.public_identity())
            .expect("local recipient record is installed during construction")
    }

    pub fn accept_recipient_record(
        &mut self,
        bytes: &[u8],
        current_members: &[crate::workspace_domain::Member],
    ) -> Result<(), ConversationAuthorityError> {
        let exact = ExactRecordV1::decode(bytes)?;
        let ConversationRecordV1::RecipientKey(record) = exact.record() else {
            return Err(
                ConversationError::UnauthorizedData("record is not a recipient key").into(),
            );
        };
        let identity = PublicIdentity::from_bytes(record.installation);
        if !current_members
            .iter()
            .any(|member| member.public_identity == identity)
        {
            return Err(ConversationError::UnauthorizedData(
                "recipient key does not belong to a current workspace member",
            )
            .into());
        }
        self.stage_recipient_record(bytes, identity)?;
        self.store.record_recipient_key(&exact, true)?;
        Ok(())
    }

    pub fn stage_recipient_record(
        &mut self,
        bytes: &[u8],
        expected_installation: PublicIdentity,
    ) -> Result<(), ConversationAuthorityError> {
        let exact = ExactRecordV1::decode(bytes)?;
        let ConversationRecordV1::RecipientKey(record) = exact.record() else {
            return Err(
                ConversationError::UnauthorizedData("record is not a recipient key").into(),
            );
        };
        let identity = PublicIdentity::from_bytes(record.installation);
        if record.workspace_id != self.workspace_id || identity != expected_installation {
            return Err(
                ConversationError::UnauthorizedData("recipient-key bootstrap binding").into(),
            );
        }
        self.store.record_recipient_key(&exact, false)?;
        self.recipient_records.insert(identity, exact);
        Ok(())
    }

    pub fn commit_transition(
        &mut self,
        prepared: &PreparedMembershipTransition,
    ) -> Result<CommittedMembershipEpoch, ConversationAuthorityError> {
        self.commit_transition_with(prepared, &mut OperatingSystemEpochMaterial)
    }

    pub fn commit_transition_with(
        &mut self,
        prepared: &PreparedMembershipTransition,
        material: &mut impl EpochMaterialProvider,
    ) -> Result<CommittedMembershipEpoch, ConversationAuthorityError> {
        if prepared.operation.operation.workspace_id.as_bytes() != hex_bytes(self.workspace_id) {
            return Err(ConversationAuthorityError::InvalidPreparedTransition(
                "workspace does not match",
            ));
        }
        if prepared.author != self.identity.public_identity()
            || !prepared
                .resulting_members
                .iter()
                .any(|member| member.public_identity == prepared.author)
        {
            return Err(ConversationAuthorityError::UnauthorizedCoordinator);
        }
        let previous_head = prepared
            .before_head
            .as_ref()
            .map(|head| decode_membership_head(head.as_str()))
            .transpose()?;
        let resulting_head = decode_membership_head(prepared.resulting_head.as_str())?;

        if let Some(existing) = self.store.exact_epoch_for_head(&resulting_head)? {
            let epoch = ExactRecordV1::decode(&existing)?;
            self.open_own_envelope(&epoch, material)?;
            return Ok(CommittedMembershipEpoch {
                epoch,
                genesis_channel: None,
                lookup_peer_set_version: self.store.lookup_peer_set_version()?,
            });
        }

        let recipient_inputs = prepared
            .resulting_members
            .iter()
            .map(|member| {
                let exact = self.recipient_records.get(&member.public_identity).ok_or(
                    ConversationAuthorityError::MissingRecipientKey(member.public_identity),
                )?;
                let ConversationRecordV1::RecipientKey(record) = exact.record() else {
                    return Err(ConversationAuthorityError::InvalidPreparedTransition(
                        "recipient directory contains the wrong record family",
                    ));
                };
                validate_recipient_binding(record, self.workspace_id, member.public_identity)?;
                Ok((
                    member,
                    exact,
                    RecipientPublicKey::from_bytes(record.recipient_public_key)?,
                ))
            })
            .collect::<Result<Vec<_>, ConversationAuthorityError>>()?;
        let epoch_key = material.generate_epoch_key()?;
        let mut recipients = Vec::with_capacity(recipient_inputs.len());
        for (member, exact, public) in recipient_inputs {
            let context = EpochEnvelopeContextV1 {
                workspace_id: self.workspace_id,
                previous_membership_head: previous_head,
                resulting_membership_head: resulting_head,
                coordinator: *self.identity.public_identity().as_bytes(),
                recipient: *member.public_identity.as_bytes(),
                recipient_public_key: public,
                recipient_key_record_id: *exact.id(),
            };
            let envelope = material.wrap_epoch_key(&epoch_key, &context)?;
            recipients.push(EpochRecipientV1 {
                member: *member.public_identity.as_bytes(),
                recipient_key_record_id: *exact.id(),
                encapsulation: envelope.encapsulation,
                wrapped_epoch_key: envelope.ciphertext,
            });
        }
        recipients.sort_by_key(|entry| entry.member);
        if recipients
            .windows(2)
            .any(|pair| pair[0].member >= pair[1].member)
        {
            return Err(ConversationAuthorityError::InvalidPreparedTransition(
                "resulting member set is not unique",
            ));
        }
        let epoch = ExactRecordV1::author(
            ConversationRecordV1::Epoch(EpochRecordV1 {
                workspace_id: self.workspace_id,
                previous_membership_head: previous_head,
                resulting_membership_head: resulting_head,
                coordinator: *self.identity.public_identity().as_bytes(),
                suite: EPOCH_ENVELOPE_SUITE_V1,
                recipients,
            }),
            &self.identity,
        )?;
        let opened = self.open_own_envelope(&epoch, material)?;
        if opened != epoch_key {
            return Err(ConversationError::AuthenticatedOpen.into());
        }

        let genesis_channel = if prepared.before_head.is_none() {
            Some(author_general_channel(
                &self.identity,
                self.workspace_id,
                resulting_head,
                transition_time(&prepared.operation.operation.body),
            )?)
        } else {
            None
        };
        let processed_request_id = match &prepared.operation.operation.body {
            MembershipOperationBody::RemoveMember {
                authorization: RemovalAuthorizationV1::MemberRequest(request),
                ..
            } => Some(request.request_id().map_err(|_| {
                ConversationAuthorityError::InvalidPreparedTransition("request ID is invalid")
            })?),
            _ => None,
        };
        let version = self
            .store
            .commit_membership_epoch(&AtomicMembershipEpochCommit {
                operation_id: prepared.operation_id.clone(),
                exact_membership_operation: prepared.exact_operation.clone(),
                resulting_members: prepared.resulting_members.clone(),
                previous_membership_head: previous_head,
                resulting_membership_head: resulting_head,
                coordinator: *self.identity.public_identity().as_bytes(),
                exact_epoch_record: epoch.bytes().to_vec(),
                epoch_record_id: *epoch.id(),
                coordinator_own_envelope_opened: true,
                processed_request_id,
                genesis_channel: genesis_channel.clone(),
            })?;
        Ok(CommittedMembershipEpoch {
            epoch,
            genesis_channel,
            lookup_peer_set_version: version,
        })
    }

    fn open_own_envelope(
        &self,
        epoch: &ExactRecordV1,
        material: &mut impl EpochMaterialProvider,
    ) -> Result<EpochKey, ConversationAuthorityError> {
        let ConversationRecordV1::Epoch(record) = epoch.record() else {
            return Err(ConversationAuthorityError::InvalidPreparedTransition(
                "persisted epoch has the wrong family",
            ));
        };
        let own_identity = self.identity.public_identity();
        let recipient = record
            .recipients
            .iter()
            .find(|recipient| recipient.member == *own_identity.as_bytes())
            .ok_or(ConversationAuthorityError::UnauthorizedCoordinator)?;
        let key_record = self.recipient_records.get(&own_identity).ok_or(
            ConversationAuthorityError::MissingRecipientKey(own_identity),
        )?;
        let ConversationRecordV1::RecipientKey(key_record) = key_record.record() else {
            return Err(ConversationAuthorityError::InvalidPreparedTransition(
                "local recipient record has the wrong family",
            ));
        };
        let context = EpochEnvelopeContextV1 {
            workspace_id: record.workspace_id,
            previous_membership_head: record.previous_membership_head,
            resulting_membership_head: record.resulting_membership_head,
            coordinator: record.coordinator,
            recipient: recipient.member,
            recipient_public_key: RecipientPublicKey::from_bytes(key_record.recipient_public_key)?,
            recipient_key_record_id: recipient.recipient_key_record_id,
        };
        material
            .open_epoch_key(
                &HpkeEpochEnvelopeV1 {
                    encapsulation: recipient.encapsulation,
                    ciphertext: recipient.wrapped_epoch_key,
                },
                self.local_recipient.private(),
                &context,
            )
            .map_err(Into::into)
    }
}

fn validate_recipient_binding(
    record: &RecipientKeyRecordV1,
    workspace: WorkspaceId,
    member: PublicIdentity,
) -> Result<(), ConversationAuthorityError> {
    if record.workspace_id != workspace
        || record.installation != *member.as_bytes()
        || record.suite != EPOCH_ENVELOPE_SUITE_V1
        || record.generation != 0
    {
        return Err(ConversationError::UnauthorizedData("recipient-key binding").into());
    }
    Ok(())
}

fn author_general_channel(
    identity: &InstallationIdentity,
    workspace_id: WorkspaceId,
    epoch: MembershipHead,
    created_at: i64,
) -> Result<ExactRecordV1, ConversationError> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"resonance.general-channel.v1\0");
    hasher.update(&workspace_id);
    let mut channel_id = [0; 16];
    channel_id.copy_from_slice(&hasher.finalize().as_bytes()[..16]);
    ExactRecordV1::author(
        ConversationRecordV1::Channel(ChannelRecordV1 {
            workspace_id,
            channel_id,
            authorization_epoch: epoch,
            creator: *identity.public_identity().as_bytes(),
            author: *identity.public_identity().as_bytes(),
            author_sequence: 0,
            created_at,
            predecessor: None,
            operation: ChannelOperationV1::Create {
                name: "#general".to_owned(),
            },
        }),
        identity,
    )
}

fn transition_time(body: &MembershipOperationBody) -> i64 {
    match body {
        MembershipOperationBody::AddMember { added_at, .. } => *added_at,
        MembershipOperationBody::RemoveMember { removed_at, .. } => *removed_at,
    }
}

fn decode_membership_head(value: &str) -> Result<MembershipHead, ConversationAuthorityError> {
    decode_hex_32(value).ok_or(ConversationAuthorityError::InvalidPreparedTransition(
        "membership head is not lowercase hexadecimal",
    ))
}

fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut output = [0; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn hex_bytes(value: [u8; 32]) -> [u8; 64] {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = [0; 64];
    for (index, byte) in value.into_iter().enumerate() {
        output[index * 2] = HEX[usize::from(byte >> 4)];
        output[index * 2 + 1] = HEX[usize::from(byte & 0x0f)];
    }
    output
}
