//! Validated direct-address candidates kept separate from membership authority.

use std::{collections::BTreeMap, net::SocketAddr};

use crate::{identity::PublicIdentity, membership_log::MembershipLog};

use super::{
    channels::membership_id,
    wire::{ConversationRecordV1, ExactRecordV1, RecordId},
    ConversationError,
};

pub(crate) const ADDRESS_NOTICE_MAX_TTL_SECONDS: i64 = 300;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DirectPeerRoute {
    pub identity: PublicIdentity,
    pub candidates: Vec<SocketAddr>,
}

#[derive(Clone, Debug)]
struct Notice {
    generation: u64,
    expires_at: i64,
    record_id: RecordId,
    candidates: Vec<SocketAddr>,
    next_candidate: usize,
}

#[derive(Clone, Default)]
pub(crate) struct AddressDirectory {
    notices: BTreeMap<PublicIdentity, Notice>,
}

impl AddressDirectory {
    pub(crate) fn accept(
        &mut self,
        bytes: &[u8],
        membership: &MembershipLog,
        workspace_id: &str,
        now: i64,
    ) -> Result<bool, ConversationError> {
        let exact = ExactRecordV1::decode(bytes)?;
        let ConversationRecordV1::AddressNotice(record) = exact.record() else {
            return Err(ConversationError::UnauthorizedData(
                "control record is not an address notice",
            ));
        };
        let sender = PublicIdentity::from_bytes(record.sender);
        let projection = membership.projection(workspace_id);
        if !projection.contains(&sender)
            || encode_hex(&record.workspace_id) != workspace_id
            || membership_id(&record.observed_membership_head).as_ref()
                != projection.canonical_head.as_ref()
        {
            return Err(ConversationError::UnauthorizedData(
                "address notice is not bound to current membership",
            ));
        }
        if record.expires_at <= now
            || record.expires_at > now.saturating_add(ADDRESS_NOTICE_MAX_TTL_SECONDS)
        {
            return Err(ConversationError::UnauthorizedData(
                "address notice expiry is outside the accepted window",
            ));
        }
        let mut candidates = Vec::with_capacity(record.addresses.len());
        for candidate in &record.addresses {
            let address = candidate
                .parse::<SocketAddr>()
                .map_err(|_| ConversationError::MalformedBytes("direct socket address"))?;
            if address.port() == 0 || address.ip().is_unspecified() || address.ip().is_multicast() {
                return Err(ConversationError::UnauthorizedData(
                    "address notice contains an unusable candidate",
                ));
            }
            candidates.push(address);
        }
        candidates.sort();
        candidates.dedup();
        if candidates.is_empty() {
            return Err(ConversationError::UnauthorizedData(
                "address notice has no candidate",
            ));
        }
        if let Some(existing) = self.notices.get(&sender) {
            if existing.record_id == *exact.id() {
                return Ok(false);
            }
            if record.generation <= existing.generation {
                return Err(ConversationError::UnauthorizedData(
                    "address notice generation is stale",
                ));
            }
        }
        self.notices.insert(
            sender,
            Notice {
                generation: record.generation,
                expires_at: record.expires_at,
                record_id: *exact.id(),
                candidates,
                next_candidate: 0,
            },
        );
        Ok(true)
    }

    pub(crate) fn routes(
        &mut self,
        membership: &MembershipLog,
        workspace_id: &str,
        now: i64,
    ) -> Vec<DirectPeerRoute> {
        let projection = membership.projection(workspace_id);
        self.notices
            .retain(|identity, notice| projection.contains(identity) && notice.expires_at > now);
        self.notices
            .iter()
            .map(|(identity, notice)| {
                let mut candidates = notice.candidates.clone();
                let candidate_count = candidates.len();
                candidates.rotate_left(notice.next_candidate % candidate_count);
                DirectPeerRoute {
                    identity: *identity,
                    candidates,
                }
            })
            .collect()
    }

    pub(crate) fn rotate_candidates(&mut self) {
        for notice in self.notices.values_mut() {
            notice.next_candidate = (notice.next_candidate + 1) % notice.candidates.len();
        }
    }

    pub(crate) fn has_candidate_for_other_member(
        &mut self,
        membership: &MembershipLog,
        workspace_id: &str,
        local_identity: PublicIdentity,
        now: i64,
    ) -> bool {
        self.routes(membership, workspace_id, now)
            .iter()
            .any(|route| route.identity != local_identity)
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(DIGITS[(byte >> 4) as usize] as char);
        output.push(DIGITS[(byte & 0x0f) as usize] as char);
    }
    output
}
