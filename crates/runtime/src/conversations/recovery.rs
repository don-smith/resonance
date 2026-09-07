//! Sparse recovery state derived from exact possession and canonical membership history.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    identity::PublicIdentity,
    membership_log::MembershipLog,
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

use super::{
    channels::membership_id,
    wire::{
        MessageRecordV1, PublicIdentityBytes, MAX_MEMBERS_PER_EPOCH, MAX_RECOVERY_RANGES,
        MAX_RECOVERY_RESPONSE_RECORDS,
    },
};

const RECOVERY_RETRY_SECONDS: i64 = 2;

#[derive(Clone, Copy, Debug, Default)]
struct PeerRecoveryRequestState {
    initial_sync_pending: bool,
    in_flight_since: Option<i64>,
    timed_out_attempts: u8,
    continuation_pending: bool,
}

/// Bounded request suppression for direct recovery peers.
///
/// A newly observed peer receives one initial synchronization request. Afterwards, requests
/// resume only for sparse gaps or after a timed-out in-flight request. Successful responses clear
/// the in-flight marker; a full response page permits one continuation request to obtain the
/// required empty completion response.
#[derive(Default)]
pub struct RecoveryRequestTracker {
    peers: BTreeMap<PublicIdentity, PeerRecoveryRequestState>,
    membership_head: Option<[u8; 32]>,
}

impl RecoveryRequestTracker {
    pub fn reset(&mut self) {
        self.peers.clear();
        self.membership_head = None;
    }

    pub fn reset_for_membership(&mut self, membership_head: Option<[u8; 32]>) {
        if self.membership_head != membership_head {
            self.peers.clear();
            self.membership_head = membership_head;
        }
    }

    pub fn retain_peers(&mut self, peers: &[PublicIdentity]) {
        let peers = peers.iter().copied().collect::<BTreeSet<_>>();
        self.peers.retain(|peer, _| peers.contains(peer));
    }

    pub fn should_request(
        &mut self,
        peer: PublicIdentity,
        has_sparse_gaps: bool,
        now: i64,
    ) -> bool {
        if !self.peers.contains_key(&peer) && self.peers.len() >= MAX_MEMBERS_PER_EPOCH {
            return false;
        }
        let state = self.peers.entry(peer).or_insert(PeerRecoveryRequestState {
            initial_sync_pending: true,
            ..PeerRecoveryRequestState::default()
        });
        if let Some(sent_at) = state.in_flight_since {
            let retry_after = RECOVERY_RETRY_SECONDS
                .saturating_mul(1_i64 << u32::from(state.timed_out_attempts.min(4)));
            if now.saturating_sub(sent_at) < retry_after {
                return false;
            }
            state.in_flight_since = None;
            state.timed_out_attempts = state.timed_out_attempts.saturating_add(1);
        }
        if !state.initial_sync_pending && !has_sparse_gaps && !state.continuation_pending {
            return false;
        }
        state.in_flight_since = Some(now);
        true
    }

    pub fn response_completed(&mut self, peer: PublicIdentity, records_received: usize) {
        let state = self.peers.entry(peer).or_insert(PeerRecoveryRequestState {
            initial_sync_pending: true,
            ..PeerRecoveryRequestState::default()
        });
        state.initial_sync_pending = false;
        state.in_flight_since = None;
        state.timed_out_attempts = 0;
        state.continuation_pending = records_received == MAX_RECOVERY_RESPONSE_RECORDS;
    }
}

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
