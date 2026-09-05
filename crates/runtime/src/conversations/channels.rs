//! Deterministic replay for creator-owned public channel lifecycle chains.

use std::collections::{BTreeMap, BTreeSet};

use unicode_normalization::UnicodeNormalization as _;

use crate::{
    identity::PublicIdentity,
    membership_log::{MembershipLog, MembershipOperationId},
};

use super::{
    wire::{
        ChannelId, ChannelOperationV1, ChannelRecordV1, ExactRecordV1, MembershipHead, RecordId,
    },
    ConversationError,
};

pub const MAX_DIAGNOSTIC_RECORDS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelView {
    pub channel_id: ChannelId,
    pub name: String,
    pub creator: PublicIdentity,
    pub archived: bool,
    pub head: RecordId,
}

#[derive(Clone, Default)]
pub(crate) struct ChannelProjection {
    channels: BTreeMap<ChannelId, ChannelView>,
    chains: BTreeMap<ChannelId, Vec<ExactRecordV1>>,
    records: Vec<ExactRecordV1>,
    workspace_bytes: [u8; 32],
    rejected_ids: BTreeSet<RecordId>,
    diagnostic_ids: Vec<RecordId>,
}

impl ChannelProjection {
    pub(crate) fn replay(
        records: &[ExactRecordV1],
        membership: &MembershipLog,
        workspace_id: &str,
        workspace_bytes: &[u8; 32],
    ) -> Self {
        let mut candidates = BTreeMap::<ChannelId, Vec<ExactRecordV1>>::new();
        let mut diagnostics = BTreeSet::new();
        for exact in records {
            let super::wire::ConversationRecordV1::Channel(record) = exact.record() else {
                continue;
            };
            if record.workspace_id != *workspace_bytes
                || !valid_channel_name_for_operation(&record.operation)
                || !member_at_epoch(
                    membership,
                    workspace_id,
                    &record.authorization_epoch,
                    record.author,
                )
            {
                diagnostics.insert(*exact.id());
                continue;
            }
            candidates
                .entry(record.channel_id)
                .or_default()
                .push(exact.clone());
        }

        let mut channels = BTreeMap::new();
        let mut chains = BTreeMap::new();
        for (channel_id, mut records) in candidates {
            records.sort_by_key(|record| *record.id());
            let Some(create) = records.iter().find(|exact| {
                matches!(
                    exact.record(),
                    super::wire::ConversationRecordV1::Channel(ChannelRecordV1 {
                        operation: ChannelOperationV1::Create { .. },
                        predecessor: None,
                        creator,
                        author,
                        ..
                    }) if creator == author
                )
            }) else {
                diagnostics.extend(records.iter().map(|record| *record.id()));
                continue;
            };
            let super::wire::ConversationRecordV1::Channel(create_record) = create.record() else {
                unreachable!("channel candidate family was checked");
            };
            let creator = create_record.creator;
            let mut chain = vec![create.clone()];
            let mut head = *create.id();
            let mut archived = false;
            loop {
                let super::wire::ConversationRecordV1::Channel(parent_record) =
                    chain.last().expect("channel chain has create").record()
                else {
                    unreachable!("channel chain family was checked");
                };
                let mut children = records
                    .iter()
                    .filter(|exact| {
                        let super::wire::ConversationRecordV1::Channel(record) = exact.record()
                        else {
                            return false;
                        };
                        record.predecessor == Some(head)
                            && record.creator == creator
                            && record.author == creator
                            && epoch_descends_or_equals(
                                membership,
                                workspace_id,
                                &record.authorization_epoch,
                                &parent_record.authorization_epoch,
                            )
                    })
                    .collect::<Vec<_>>();
                children.sort_by_key(|record| *record.id());
                let Some(child) = children.first() else {
                    break;
                };
                if archived {
                    diagnostics.insert(*child.id());
                    break;
                }
                chain.push((*child).clone());
                head = *child.id();
                let super::wire::ConversationRecordV1::Channel(record) = child.record() else {
                    unreachable!("channel child family was checked");
                };
                archived = matches!(record.operation, ChannelOperationV1::Archive);
            }
            let selected = chain
                .iter()
                .map(|record| *record.id())
                .collect::<BTreeSet<_>>();
            diagnostics.extend(
                records
                    .iter()
                    .map(|record| *record.id())
                    .filter(|id| !selected.contains(id)),
            );
            let super::wire::ConversationRecordV1::Channel(head_record) =
                chain.last().expect("channel chain has create").record()
            else {
                unreachable!("channel chain family was checked");
            };
            let name = chain
                .iter()
                .rev()
                .find_map(|exact| {
                    let super::wire::ConversationRecordV1::Channel(record) = exact.record() else {
                        return None;
                    };
                    match &record.operation {
                        ChannelOperationV1::Create { name }
                        | ChannelOperationV1::Rename { name } => Some(name.clone()),
                        ChannelOperationV1::Archive => None,
                    }
                })
                .expect("create carries a channel name");
            channels.insert(
                channel_id,
                ChannelView {
                    channel_id,
                    name,
                    creator: PublicIdentity::from_bytes(creator),
                    archived,
                    head: *chain.last().expect("channel chain has create").id(),
                },
            );
            debug_assert_eq!(head_record.channel_id, channel_id);
            chains.insert(channel_id, chain);
        }

        loop {
            let mut claims = BTreeMap::<String, Vec<(ChannelId, RecordId)>>::new();
            for channel in channels.values().filter(|channel| !channel.archived) {
                claims
                    .entry(normalized_name(&channel.name))
                    .or_default()
                    .push((channel.channel_id, channel.head));
            }
            let mut losers = Vec::new();
            for claims in claims.values_mut() {
                claims.sort_by_key(|(_, record_id)| *record_id);
                losers.extend(claims.iter().skip(1).copied());
            }
            if losers.is_empty() {
                break;
            }
            for (channel_id, record_id) in losers {
                diagnostics.insert(record_id);
                let Some(chain) = chains.get_mut(&channel_id) else {
                    continue;
                };
                if chain.last().map(ExactRecordV1::id) == Some(&record_id) {
                    chain.pop();
                }
                if chain.is_empty() {
                    chains.remove(&channel_id);
                    channels.remove(&channel_id);
                } else {
                    channels.insert(channel_id, view_from_chain(channel_id, chain));
                }
            }
        }

        Self {
            channels,
            chains,
            records: records
                .iter()
                .filter(|exact| {
                    matches!(
                        exact.record(),
                        super::wire::ConversationRecordV1::Channel(_)
                    )
                })
                .cloned()
                .collect(),
            workspace_bytes: *workspace_bytes,
            rejected_ids: diagnostics.clone(),
            diagnostic_ids: diagnostics
                .into_iter()
                .take(MAX_DIAGNOSTIC_RECORDS)
                .collect(),
        }
    }

    pub(crate) fn views(&self) -> Vec<ChannelView> {
        self.channels.values().cloned().collect()
    }

    pub(crate) fn diagnostic_ids(&self) -> &[RecordId] {
        &self.diagnostic_ids
    }

    pub(crate) fn accepted_ids(&self) -> BTreeSet<RecordId> {
        self.chains
            .values()
            .flatten()
            .map(|record| *record.id())
            .filter(|id| !self.rejected_ids.contains(id))
            .collect()
    }

    pub(crate) fn permits_message(
        &self,
        channel_id: &ChannelId,
        channel_head: &RecordId,
        epoch: &MembershipHead,
        membership: &MembershipLog,
        workspace_id: &str,
    ) -> bool {
        let Some(epoch_id) = membership_id(epoch) else {
            return false;
        };
        let Some(epoch_projection) = membership.projection_at(workspace_id, &epoch_id) else {
            return false;
        };
        let filtered = self
            .records
            .iter()
            .filter(|exact| {
                let super::wire::ConversationRecordV1::Channel(record) = exact.record() else {
                    return false;
                };
                membership_id(&record.authorization_epoch).is_some_and(|record_epoch| {
                    epoch_projection.canonical_lineage.contains(&record_epoch)
                })
            })
            .cloned()
            .collect::<Vec<_>>();
        let historical = Self::replay(&filtered, membership, workspace_id, &self.workspace_bytes);
        historical
            .channels
            .get(channel_id)
            .is_some_and(|channel| channel.head == *channel_head && !channel.archived)
    }
}

fn view_from_chain(channel_id: ChannelId, chain: &[ExactRecordV1]) -> ChannelView {
    let super::wire::ConversationRecordV1::Channel(first) = chain
        .first()
        .expect("channel chain has a create record")
        .record()
    else {
        unreachable!("channel chain family was checked");
    };
    let super::wire::ConversationRecordV1::Channel(last) =
        chain.last().expect("channel chain has a head").record()
    else {
        unreachable!("channel chain family was checked");
    };
    let name = chain
        .iter()
        .rev()
        .find_map(|exact| {
            let super::wire::ConversationRecordV1::Channel(record) = exact.record() else {
                return None;
            };
            match &record.operation {
                ChannelOperationV1::Create { name } | ChannelOperationV1::Rename { name } => {
                    Some(name.clone())
                }
                ChannelOperationV1::Archive => None,
            }
        })
        .expect("channel create carries a name");
    ChannelView {
        channel_id,
        name,
        creator: PublicIdentity::from_bytes(first.creator),
        archived: matches!(last.operation, ChannelOperationV1::Archive),
        head: *chain.last().expect("channel chain has a head").id(),
    }
}

fn valid_channel_name_for_operation(operation: &ChannelOperationV1) -> bool {
    match operation {
        ChannelOperationV1::Archive => true,
        ChannelOperationV1::Create { name } | ChannelOperationV1::Rename { name } => {
            name == name.trim()
                && name.starts_with('#')
                && name.len() > 1
                && name.nfc().eq(name.chars())
        }
    }
}

fn normalized_name(name: &str) -> String {
    name.nfc().flat_map(char::to_lowercase).collect()
}

fn epoch_descends_or_equals(
    membership: &MembershipLog,
    workspace_id: &str,
    child: &MembershipHead,
    parent: &MembershipHead,
) -> bool {
    let Some(child) = membership_id(child) else {
        return false;
    };
    let Some(parent) = membership_id(parent) else {
        return false;
    };
    membership
        .projection_at(workspace_id, &child)
        .is_some_and(|projection| projection.canonical_lineage.contains(&parent))
}

fn member_at_epoch(
    membership: &MembershipLog,
    workspace_id: &str,
    epoch: &MembershipHead,
    identity: [u8; 32],
) -> bool {
    membership_id(epoch)
        .and_then(|head| membership.projection_at(workspace_id, &head))
        .is_some_and(|projection| projection.contains(&PublicIdentity::from_bytes(identity)))
}

pub(crate) fn membership_id(bytes: &MembershipHead) -> Option<MembershipOperationId> {
    let value = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    MembershipOperationId::parse(&value).ok()
}

pub(crate) fn require_current_channel(
    projection: &ChannelProjection,
    channel_id: &ChannelId,
) -> Result<ChannelView, ConversationError> {
    projection
        .channels
        .get(channel_id)
        .filter(|channel| !channel.archived)
        .cloned()
        .ok_or(ConversationError::UnauthorizedData(
            "channel is unavailable or archived",
        ))
}
