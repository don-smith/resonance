use std::collections::BTreeMap;

use crate::identity::PublicIdentity;
use crate::workspace_domain::Member;

use super::{MembershipOperationId, RemovalAuthorizationV1};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MembershipStatus {
    Canonical,
    Pending,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipInterval {
    pub interval_id: MembershipOperationId,
    pub member: Member,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CanonicalRemoval {
    pub interval_id: MembershipOperationId,
    pub authorization: RemovalAuthorizationV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipProjection {
    pub canonical_head: Option<MembershipOperationId>,
    pub creator: Option<PublicIdentity>,
    pub members: Vec<Member>,
    pub intervals: BTreeMap<PublicIdentity, MembershipInterval>,
    pub latest_removals: BTreeMap<PublicIdentity, CanonicalRemoval>,
    pub canonical_lineage: Vec<MembershipOperationId>,
    pub statuses: BTreeMap<MembershipOperationId, MembershipStatus>,
}

impl MembershipProjection {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            canonical_head: None,
            creator: None,
            members: Vec::new(),
            intervals: BTreeMap::new(),
            latest_removals: BTreeMap::new(),
            canonical_lineage: Vec::new(),
            statuses: BTreeMap::new(),
        }
    }

    #[must_use]
    pub fn contains(&self, public_identity: &PublicIdentity) -> bool {
        self.members
            .iter()
            .any(|member| member.public_identity == *public_identity)
    }

    #[must_use]
    pub fn contains_text(&self, public_identity: &str) -> bool {
        PublicIdentity::parse(public_identity)
            .map(|identity| self.contains(&identity))
            .unwrap_or(false)
    }

    #[must_use]
    pub fn interval_id(&self, public_identity: &PublicIdentity) -> Option<&MembershipOperationId> {
        self.intervals
            .get(public_identity)
            .map(|interval| &interval.interval_id)
    }

    #[must_use]
    pub fn status(&self, operation_id: &MembershipOperationId) -> Option<&MembershipStatus> {
        self.statuses.get(operation_id)
    }
}
