//! Socket-free semantic conversation runtime over exact records and SQLite custody.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    identity::{InstallationIdentity, PublicIdentity},
    membership_log::{MembershipLog, SignedSelfRemovalRequestV1},
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

use super::{
    channels::{membership_id, require_current_channel, ChannelProjection, ChannelView},
    crypto::{
        self, EpochEnvelopeContextV1, EpochKey, HpkeEpochEnvelopeV1, MessageHeaderInputV1,
        RecipientPublicKey,
    },
    key_custody::InstallationRecipientKey,
    recovery,
    store::MessageDisposition,
    wire::{
        ChannelId, ChannelOperationV1, ChannelRecordV1, ConversationRecordV1, EpochRecordV1,
        ExactRecordV1, MembershipHead, MessageRecordV1, RecordId, WorkspaceId,
    },
    ConversationError,
};

pub use super::{recovery::SparseRecoveryRange, store::MessageCommitOutcome};

#[derive(Debug)]
pub enum ConversationRuntimeError {
    Conversation(ConversationError),
    Store(WorkspaceStoreError),
    InvalidWorkspace,
    MissingCurrentEpoch,
    LocalAuthoringBlocked,
    InvalidEpoch(&'static str),
}

impl std::fmt::Display for ConversationRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conversation(error) => write!(formatter, "conversation failed: {error}"),
            Self::Store(error) => write!(formatter, "conversation storage failed: {error}"),
            Self::InvalidWorkspace => formatter.write_str("conversation workspace is invalid"),
            Self::MissingCurrentEpoch => {
                formatter.write_str("the current conversation epoch is unavailable")
            }
            Self::LocalAuthoringBlocked => {
                formatter.write_str("local conversation authoring is blocked")
            }
            Self::InvalidEpoch(reason) => {
                write!(formatter, "conversation epoch is invalid: {reason}")
            }
        }
    }
}

impl std::error::Error for ConversationRuntimeError {}

impl From<ConversationError> for ConversationRuntimeError {
    fn from(error: ConversationError) -> Self {
        Self::Conversation(error)
    }
}

impl From<WorkspaceStoreError> for ConversationRuntimeError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Store(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationSyncState {
    Current,
    WaitingToSync,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MessageView {
    pub message_id: RecordId,
    pub channel_id: ChannelId,
    pub author: PublicIdentity,
    pub author_sequence: u64,
    pub lamport: u64,
    pub created_at: i64,
    pub markdown: String,
}

pub struct ConversationRuntime {
    identity: InstallationIdentity,
    workspace_id: String,
    workspace_bytes: WorkspaceId,
    store: WorkspaceStore,
    membership: MembershipLog,
    recipient: InstallationRecipientKey,
    recipient_records: BTreeMap<RecordId, ExactRecordV1>,
    epoch_records: BTreeMap<MembershipHead, ExactRecordV1>,
    epoch_keys: BTreeMap<MembershipHead, EpochKey>,
    channel_records: Vec<ExactRecordV1>,
    channels: ChannelProjection,
    local_authoring_blocked: bool,
}

impl ConversationRuntime {
    pub fn open(
        identity: InstallationIdentity,
        workspace_id: impl Into<String>,
        store: WorkspaceStore,
        membership: MembershipLog,
    ) -> Result<Self, ConversationRuntimeError> {
        let workspace_id = workspace_id.into();
        let workspace_bytes = decode_hex_32(&workspace_id).ok_or(Self::invalid_workspace())?;
        let recipient = InstallationRecipientKey::load(&identity);
        let mut recipient_records = BTreeMap::new();
        for bytes in store.all_recipient_key_records()? {
            let exact = ExactRecordV1::decode(&bytes)?;
            let ConversationRecordV1::RecipientKey(record) = exact.record() else {
                return Err(ConversationRuntimeError::InvalidEpoch(
                    "recipient directory contains another record family",
                ));
            };
            if record.workspace_id == workspace_bytes {
                recipient_records.insert(*exact.id(), exact);
            }
        }
        let channel_records = store
            .conversation_channel_records()?
            .into_iter()
            .map(|bytes| ExactRecordV1::decode(&bytes).map_err(Into::into))
            .collect::<Result<Vec<_>, ConversationRuntimeError>>()?;
        let channels = ChannelProjection::replay(
            &channel_records,
            &membership,
            &workspace_id,
            &workspace_bytes,
        );
        let local_authoring_blocked = departure_blocks_authoring(
            &store,
            &membership,
            &workspace_id,
            identity.public_identity(),
        )?;
        let mut runtime = Self {
            identity,
            workspace_id,
            workspace_bytes,
            store,
            membership,
            recipient,
            recipient_records,
            epoch_records: BTreeMap::new(),
            epoch_keys: BTreeMap::new(),
            channel_records,
            channels,
            local_authoring_blocked,
        };
        for bytes in runtime.store.conversation_epoch_records()? {
            let exact = ExactRecordV1::decode(&bytes)?;
            let (head, key) = runtime.validate_and_open_epoch(&exact)?;
            runtime.epoch_keys.insert(head, key);
            runtime.epoch_records.insert(head, exact);
        }
        Ok(runtime)
    }

    const fn invalid_workspace() -> ConversationRuntimeError {
        ConversationRuntimeError::InvalidWorkspace
    }

    pub fn replace_membership(
        &mut self,
        membership: MembershipLog,
    ) -> Result<(), ConversationRuntimeError> {
        let old = self.membership.projection(&self.workspace_id);
        let new = membership.projection(&self.workspace_id);
        for losing in old
            .canonical_lineage
            .iter()
            .filter(|head| !new.canonical_lineage.contains(head))
        {
            if let Some(head) = decode_hex_32(losing.as_str()) {
                self.store.invalidate_conversation_head(&head)?;
                self.epoch_keys.remove(&head);
                self.epoch_records.remove(&head);
            }
        }
        self.membership = membership;
        self.local_authoring_blocked = departure_blocks_authoring(
            &self.store,
            &self.membership,
            &self.workspace_id,
            self.identity.public_identity(),
        )?;
        let projection = ChannelProjection::replay(
            &self.channel_records,
            &self.membership,
            &self.workspace_id,
            &self.workspace_bytes,
        );
        self.store.record_channel_records(
            &self.channel_records,
            &projection.accepted_ids(),
            None,
        )?;
        self.channels = projection;
        Ok(())
    }

    pub fn channels(&self) -> Vec<ChannelView> {
        self.channels.views()
    }

    pub fn messages(
        &self,
        channel_id: &ChannelId,
    ) -> Result<Vec<MessageView>, ConversationRuntimeError> {
        let mut output = Vec::new();
        for stored in self.store.stored_messages()? {
            if stored.disposition != MessageDisposition::Accepted {
                continue;
            }
            let ConversationRecordV1::Message(record) = stored.exact.record() else {
                continue;
            };
            if &record.channel_id != channel_id {
                continue;
            }
            let Some(key) = self.epoch_keys.get(&record.authorization_epoch) else {
                continue;
            };
            output.push(MessageView {
                message_id: *stored.exact.id(),
                channel_id: record.channel_id,
                author: PublicIdentity::from_bytes(record.author),
                author_sequence: record.author_sequence,
                lamport: record.lamport,
                created_at: record.created_at,
                markdown: crypto::open_message(record, key)?,
            });
        }
        Ok(output)
    }

    pub fn create_channel(
        &mut self,
        name: impl Into<String>,
        created_at: i64,
    ) -> Result<ChannelView, ConversationRuntimeError> {
        let mut channel_id = [0; 16];
        getrandom::fill(&mut channel_id).map_err(|_| ConversationError::RandomnessUnavailable)?;
        self.author_channel_operation(
            channel_id,
            None,
            ChannelOperationV1::Create { name: name.into() },
            created_at,
        )
    }

    pub fn rename_channel(
        &mut self,
        channel_id: ChannelId,
        name: impl Into<String>,
        created_at: i64,
    ) -> Result<ChannelView, ConversationRuntimeError> {
        let current = require_current_channel(&self.channels, &channel_id)?;
        if current.creator != self.identity.public_identity() {
            return Err(ConversationError::UnauthorizedData("channel creator authority").into());
        }
        self.author_channel_operation(
            channel_id,
            Some(current.head),
            ChannelOperationV1::Rename { name: name.into() },
            created_at,
        )
    }

    pub fn archive_channel(
        &mut self,
        channel_id: ChannelId,
        created_at: i64,
    ) -> Result<ChannelView, ConversationRuntimeError> {
        let current = require_current_channel(&self.channels, &channel_id)?;
        if current.creator != self.identity.public_identity() {
            return Err(ConversationError::UnauthorizedData("channel creator authority").into());
        }
        self.author_channel_operation(
            channel_id,
            Some(current.head),
            ChannelOperationV1::Archive,
            created_at,
        )
    }

    pub fn accept_channel_record(
        &mut self,
        bytes: &[u8],
    ) -> Result<bool, ConversationRuntimeError> {
        self.accept_channel_record_internal(bytes, false)
    }

    fn accept_channel_record_internal(
        &mut self,
        bytes: &[u8],
        outbound: bool,
    ) -> Result<bool, ConversationRuntimeError> {
        let exact = match ExactRecordV1::decode(bytes) {
            Ok(exact) => exact,
            Err(error) => {
                if !bytes.is_empty() {
                    self.store.record_conversation_diagnostic(
                        bytes,
                        None,
                        "invalid channel bytes",
                    )?;
                }
                return Err(error.into());
            }
        };
        if !matches!(exact.record(), ConversationRecordV1::Channel(_)) {
            self.store.record_conversation_diagnostic(
                exact.bytes(),
                Some(exact.id()),
                "record is not a channel",
            )?;
            return Err(ConversationError::UnauthorizedData("record is not a channel").into());
        }
        let mut records = self.channel_records.clone();
        if !records.iter().any(|stored| stored.id() == exact.id()) {
            records.push(exact.clone());
        }
        let projection = ChannelProjection::replay(
            &records,
            &self.membership,
            &self.workspace_id,
            &self.workspace_bytes,
        );
        let accepted = projection.accepted_ids();
        self.store
            .record_channel_records(&records, &accepted, outbound.then_some(&exact))?;
        for diagnostic_id in projection.diagnostic_ids() {
            if let Some(record) = records.iter().find(|record| record.id() == diagnostic_id) {
                self.store.record_conversation_diagnostic(
                    record.bytes(),
                    Some(record.id()),
                    "losing or unauthorized channel record",
                )?;
            }
        }
        let selected = accepted.contains(exact.id());
        self.channel_records = records;
        self.channels = projection;
        Ok(selected)
    }

    pub fn accept_epoch_record(&mut self, bytes: &[u8]) -> Result<(), ConversationRuntimeError> {
        let exact = ExactRecordV1::decode(bytes)?;
        let (head, key) = self.validate_and_open_epoch(&exact)?;
        self.store.record_received_epoch(&exact)?;
        self.epoch_keys.insert(head, key);
        self.epoch_records.insert(head, exact);
        Ok(())
    }

    pub fn post_message(
        &mut self,
        channel_id: ChannelId,
        markdown: &str,
        created_at: i64,
    ) -> Result<MessageView, ConversationRuntimeError> {
        if self.local_authoring_blocked {
            return Err(ConversationRuntimeError::LocalAuthoringBlocked);
        }
        let projection = self.membership.projection(&self.workspace_id);
        if !projection.contains(&self.identity.public_identity()) {
            return Err(ConversationError::UnauthorizedData(
                "local identity is not a current member",
            )
            .into());
        }
        let head = projection
            .canonical_head
            .as_ref()
            .and_then(|head| decode_hex_32(head.as_str()))
            .ok_or(ConversationRuntimeError::MissingCurrentEpoch)?;
        let key = self
            .epoch_keys
            .get(&head)
            .ok_or(ConversationRuntimeError::MissingCurrentEpoch)?;
        let channel = require_current_channel(&self.channels, &channel_id)?;
        let (sequence, lamport) = self.store.local_conversation_clock()?;
        let exact = crypto::seal_message(
            &self.identity,
            key,
            MessageHeaderInputV1 {
                workspace_id: self.workspace_bytes,
                channel_id,
                authorization_epoch: head,
                channel_head: channel.head,
                author_sequence: sequence,
                lamport: lamport.saturating_add(1),
                created_at,
            },
            markdown,
        )?;
        let ConversationRecordV1::Message(record) = exact.record() else {
            unreachable!("message sealing returns a message record");
        };
        self.store
            .commit_local_message(&exact, record, sequence, lamport)?;
        Ok(MessageView {
            message_id: *exact.id(),
            channel_id,
            author: self.identity.public_identity(),
            author_sequence: sequence,
            lamport: record.lamport,
            created_at,
            markdown: markdown.to_owned(),
        })
    }

    pub fn accept_message_record(
        &mut self,
        bytes: &[u8],
    ) -> Result<MessageCommitOutcome, ConversationRuntimeError> {
        let exact = ExactRecordV1::decode(bytes)?;
        let ConversationRecordV1::Message(record) = exact.record() else {
            return Err(ConversationError::UnauthorizedData("record is not a message").into());
        };
        self.validate_message(record)?;
        let key = self
            .epoch_keys
            .get(&record.authorization_epoch)
            .ok_or(ConversationError::MissingKey)?;
        crypto::open_message(record, key)?;
        Ok(self.store.commit_received_message(&exact, record)?)
    }

    pub fn mark_read(
        &self,
        channel_id: ChannelId,
        message_id: RecordId,
    ) -> Result<(), ConversationRuntimeError> {
        let stored = self
            .store
            .stored_messages()?
            .into_iter()
            .find(|stored| stored.exact.id() == &message_id)
            .ok_or(ConversationError::UnauthorizedData(
                "message does not exist",
            ))?;
        let ConversationRecordV1::Message(record) = stored.exact.record() else {
            return Err(ConversationError::UnauthorizedData("record is not a message").into());
        };
        if record.channel_id != channel_id || stored.disposition != MessageDisposition::Accepted {
            return Err(
                ConversationError::UnauthorizedData("message is not accepted in channel").into(),
            );
        }
        self.store
            .mark_conversation_read(&channel_id, record, &message_id)?;
        Ok(())
    }

    pub fn unread_count(&self, channel_id: ChannelId) -> Result<usize, ConversationRuntimeError> {
        let position = self.store.conversation_read_position(&channel_id)?;
        Ok(self
            .store
            .stored_messages()?
            .into_iter()
            .filter(|stored| stored.disposition == MessageDisposition::Accepted)
            .filter_map(|stored| {
                let ConversationRecordV1::Message(record) = stored.exact.record() else {
                    return None;
                };
                (record.channel_id == channel_id).then_some((
                    record.lamport,
                    record.author,
                    record.author_sequence,
                    *stored.exact.id(),
                ))
            })
            .filter(|order| position.is_none_or(|position| *order > position))
            .count())
    }

    pub fn recovery_gaps(
        &self,
    ) -> Result<BTreeMap<[u8; 32], Vec<SparseRecoveryRange>>, ConversationRuntimeError> {
        Ok(recovery::gaps(&self.store)?)
    }

    pub fn synchronization_state(&self) -> Result<ConversationSyncState, ConversationRuntimeError> {
        let projection = self.membership.projection(&self.workspace_id);
        let pending = !self.store.conversation_outbox()?.is_empty();
        let waiting_for_epoch = projection
            .canonical_head
            .as_ref()
            .and_then(|head| decode_hex_32(head.as_str()))
            .is_some_and(|head| !self.epoch_keys.contains_key(&head));
        if self.local_authoring_blocked
            || waiting_for_epoch
            || (projection.members.len() > 1 && pending)
        {
            Ok(ConversationSyncState::WaitingToSync)
        } else {
            Ok(ConversationSyncState::Current)
        }
    }

    pub fn diagnostic_count(&self) -> Result<usize, ConversationRuntimeError> {
        Ok(self.store.conversation_diagnostic_count()?)
    }

    pub(crate) fn seal_fixture_message(
        &self,
        channel_id: ChannelId,
        markdown: &str,
        author_sequence: u64,
        lamport: u64,
        created_at: i64,
        nonce: [u8; 24],
    ) -> Result<ExactRecordV1, ConversationRuntimeError> {
        let channel = require_current_channel(&self.channels, &channel_id)?;
        self.seal_fixture_message_for_head(
            channel_id,
            channel.head,
            markdown,
            author_sequence,
            lamport,
            created_at,
            nonce,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn seal_fixture_message_for_head(
        &self,
        channel_id: ChannelId,
        channel_head: RecordId,
        markdown: &str,
        author_sequence: u64,
        lamport: u64,
        created_at: i64,
        nonce: [u8; 24],
    ) -> Result<ExactRecordV1, ConversationRuntimeError> {
        let projection = self.membership.projection(&self.workspace_id);
        let head = projection
            .canonical_head
            .as_ref()
            .and_then(|head| decode_hex_32(head.as_str()))
            .ok_or(ConversationRuntimeError::MissingCurrentEpoch)?;
        let key = self
            .epoch_keys
            .get(&head)
            .ok_or(ConversationRuntimeError::MissingCurrentEpoch)?;
        Ok(super::crypto::seal_message_with_nonce(
            &self.identity,
            key,
            MessageHeaderInputV1 {
                workspace_id: self.workspace_bytes,
                channel_id,
                authorization_epoch: head,
                channel_head,
                author_sequence,
                lamport,
                created_at,
            },
            markdown,
            nonce,
        )?)
    }

    pub(crate) fn outbox_exact_records(&self) -> Result<Vec<Vec<u8>>, ConversationRuntimeError> {
        Ok(self.store.conversation_outbox()?)
    }

    pub(crate) fn records_eligible_for(
        &self,
        requester: PublicIdentity,
    ) -> Result<Vec<Vec<u8>>, ConversationRuntimeError> {
        Ok(self
            .store
            .stored_messages()?
            .into_iter()
            .filter(|stored| stored.disposition == MessageDisposition::Accepted)
            .filter_map(|stored| {
                let ConversationRecordV1::Message(record) = stored.exact.record() else {
                    return None;
                };
                recovery::requester_is_eligible(
                    &self.membership,
                    &self.workspace_id,
                    requester,
                    record,
                )
                .then(|| stored.exact.bytes().to_vec())
            })
            .collect())
    }

    fn author_channel_operation(
        &mut self,
        channel_id: ChannelId,
        predecessor: Option<RecordId>,
        operation: ChannelOperationV1,
        created_at: i64,
    ) -> Result<ChannelView, ConversationRuntimeError> {
        if self.local_authoring_blocked {
            return Err(ConversationRuntimeError::LocalAuthoringBlocked);
        }
        let membership = self.membership.projection(&self.workspace_id);
        if !membership.contains(&self.identity.public_identity()) {
            return Err(ConversationError::UnauthorizedData(
                "local identity is not a current member",
            )
            .into());
        }
        let epoch = membership
            .canonical_head
            .as_ref()
            .and_then(|head| decode_hex_32(head.as_str()))
            .ok_or(ConversationRuntimeError::MissingCurrentEpoch)?;
        if !self.epoch_keys.contains_key(&epoch) {
            return Err(ConversationRuntimeError::MissingCurrentEpoch);
        }
        let author_sequence = self
            .channel_records
            .iter()
            .filter_map(|exact| {
                let ConversationRecordV1::Channel(record) = exact.record() else {
                    return None;
                };
                (record.author == *self.identity.public_identity().as_bytes())
                    .then_some(record.author_sequence)
            })
            .max()
            .map_or(0, |sequence| sequence.saturating_add(1));
        let exact = ExactRecordV1::author(
            ConversationRecordV1::Channel(ChannelRecordV1 {
                workspace_id: self.workspace_bytes,
                channel_id,
                authorization_epoch: epoch,
                creator: *self.identity.public_identity().as_bytes(),
                author: *self.identity.public_identity().as_bytes(),
                author_sequence,
                created_at,
                predecessor,
                operation,
            }),
            &self.identity,
        )?;
        if !self.accept_channel_record_internal(exact.bytes(), true)? {
            return Err(ConversationError::UnauthorizedData(
                "channel record lost deterministic replay",
            )
            .into());
        }
        self.channels
            .views()
            .into_iter()
            .find(|channel| channel.channel_id == channel_id)
            .ok_or(ConversationError::UnauthorizedData("channel is not active").into())
    }

    fn validate_and_open_epoch(
        &self,
        exact: &ExactRecordV1,
    ) -> Result<(MembershipHead, EpochKey), ConversationRuntimeError> {
        let ConversationRecordV1::Epoch(record) = exact.record() else {
            return Err(ConversationRuntimeError::InvalidEpoch("record family"));
        };
        self.validate_epoch(record)?;
        let local = self.identity.public_identity();
        let recipient = record
            .recipients
            .iter()
            .find(|recipient| recipient.member == *local.as_bytes())
            .ok_or(ConversationRuntimeError::InvalidEpoch(
                "local installation is not an epoch recipient",
            ))?;
        let key_record = self
            .recipient_records
            .get(&recipient.recipient_key_record_id)
            .ok_or(ConversationRuntimeError::InvalidEpoch(
                "recipient key record is missing",
            ))?;
        let ConversationRecordV1::RecipientKey(key_record) = key_record.record() else {
            return Err(ConversationRuntimeError::InvalidEpoch(
                "recipient key family",
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
        let key = crypto::unwrap_epoch_key(
            &HpkeEpochEnvelopeV1 {
                encapsulation: recipient.encapsulation,
                ciphertext: recipient.wrapped_epoch_key,
            },
            self.recipient.private(),
            &context,
        )?;
        Ok((record.resulting_membership_head, key))
    }

    fn validate_epoch(&self, record: &EpochRecordV1) -> Result<(), ConversationRuntimeError> {
        if record.workspace_id != self.workspace_bytes {
            return Err(ConversationRuntimeError::InvalidEpoch("workspace"));
        }
        let resulting_id = membership_id(&record.resulting_membership_head).ok_or(
            ConversationRuntimeError::InvalidEpoch("resulting membership head"),
        )?;
        let projection = self
            .membership
            .projection_at(&self.workspace_id, &resulting_id)
            .ok_or(ConversationRuntimeError::InvalidEpoch(
                "non-canonical membership head",
            ))?;
        if self.membership.operation_author(&resulting_id)
            != Some(PublicIdentity::from_bytes(record.coordinator))
        {
            return Err(ConversationRuntimeError::InvalidEpoch("coordinator"));
        }
        let expected_previous = projection
            .canonical_lineage
            .iter()
            .rev()
            .nth(1)
            .map(|head| {
                decode_hex_32(head.as_str()).ok_or(ConversationRuntimeError::InvalidEpoch(
                    "canonical previous membership head",
                ))
            })
            .transpose()?;
        if record.previous_membership_head != expected_previous {
            return Err(ConversationRuntimeError::InvalidEpoch(
                "previous membership head",
            ));
        }
        let expected_members = projection
            .members
            .iter()
            .map(|member| *member.public_identity.as_bytes())
            .collect::<BTreeSet<_>>();
        let actual_members = record
            .recipients
            .iter()
            .map(|recipient| recipient.member)
            .collect::<BTreeSet<_>>();
        if expected_members != actual_members || actual_members.len() != record.recipients.len() {
            return Err(ConversationRuntimeError::InvalidEpoch("recipient set"));
        }
        for recipient in &record.recipients {
            let exact = self
                .recipient_records
                .get(&recipient.recipient_key_record_id)
                .ok_or(ConversationRuntimeError::InvalidEpoch(
                    "recipient key record",
                ))?;
            let ConversationRecordV1::RecipientKey(key) = exact.record() else {
                return Err(ConversationRuntimeError::InvalidEpoch(
                    "recipient key family",
                ));
            };
            if key.workspace_id != self.workspace_bytes
                || key.installation != recipient.member
                || key.generation != 0
            {
                return Err(ConversationRuntimeError::InvalidEpoch(
                    "recipient key binding",
                ));
            }
        }
        Ok(())
    }

    fn validate_message(&self, record: &MessageRecordV1) -> Result<(), ConversationRuntimeError> {
        if record.workspace_id != self.workspace_bytes {
            return Err(ConversationError::UnauthorizedData("wrong workspace").into());
        }
        let epoch = membership_id(&record.authorization_epoch)
            .and_then(|head| self.membership.projection_at(&self.workspace_id, &head))
            .ok_or(ConversationError::UnauthorizedData(
                "non-canonical message epoch",
            ))?;
        if !epoch.contains(&PublicIdentity::from_bytes(record.author)) {
            return Err(
                ConversationError::UnauthorizedData("author was not a member at epoch").into(),
            );
        }
        if !self.channels.permits_message(
            &record.channel_id,
            &record.channel_head,
            &record.authorization_epoch,
            &self.membership,
            &self.workspace_id,
        ) {
            return Err(
                ConversationError::UnauthorizedData("stale or archived channel head").into(),
            );
        }
        Ok(())
    }
}

fn departure_blocks_authoring(
    store: &WorkspaceStore,
    membership: &MembershipLog,
    workspace_id: &str,
    local_identity: PublicIdentity,
) -> Result<bool, ConversationRuntimeError> {
    let current = membership.projection(workspace_id);
    let current_interval = current.interval_id(&local_identity);
    for bytes in store.pending_departure_requests()? {
        let request = SignedSelfRemovalRequestV1::decode(&bytes)
            .map_err(|_| ConversationRuntimeError::InvalidEpoch("persisted departure request"))?;
        if request.request.requester == *local_identity.as_bytes()
            && current_interval.map(|interval| interval.as_str())
                == Some(request.request.membership_interval_id.as_str())
        {
            return Ok(true);
        }
    }
    Ok(false)
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
