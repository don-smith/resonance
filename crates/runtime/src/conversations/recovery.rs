//! Sparse recovery state derived from exact possession and canonical membership history.

use std::collections::BTreeMap;

use crate::{
    identity::PublicIdentity,
    membership_log::MembershipLog,
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

use super::{
    channels::membership_id,
    wire::{MessageRecordV1, PublicIdentityBytes, MAX_RECOVERY_RANGES},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SparseRecoveryRange {
    pub first_sequence: u64,
    pub last_sequence: u64,
}

pub(crate) fn gaps(
    store: &WorkspaceStore,
) -> Result<BTreeMap<PublicIdentityBytes, Vec<SparseRecoveryRange>>, WorkspaceStoreError> {
    Ok(store
        .sparse_recovery_gaps(MAX_RECOVERY_RANGES)?
        .into_iter()
        .map(|(author, ranges)| {
            (
                author,
                ranges
                    .into_iter()
                    .map(|(first_sequence, last_sequence)| SparseRecoveryRange {
                        first_sequence,
                        last_sequence,
                    })
                    .collect(),
            )
        })
        .collect())
}

pub(crate) fn requester_is_eligible(
    membership: &MembershipLog,
    workspace_id: &str,
    requester: PublicIdentity,
    message: &MessageRecordV1,
) -> bool {
    membership_id(&message.authorization_epoch)
        .and_then(|head| membership.projection_at(workspace_id, &head))
        .is_some_and(|projection| {
            projection.contains(&requester)
                && projection.contains(&PublicIdentity::from_bytes(message.author))
        })
}
