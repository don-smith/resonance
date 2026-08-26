use std::collections::BTreeMap;

use crate::identity::PublicIdentity;
use crate::workspace_domain::Member;

use super::MembershipOperationId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MembershipStatus {
    Canonical,
    Pending,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipProjection {
    pub canonical_head: Option<MembershipOperationId>,
    pub members: Vec<Member>,
    pub statuses: BTreeMap<MembershipOperationId, MembershipStatus>,
}

impl MembershipProjection {
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
    pub fn status(&self, operation_id: &MembershipOperationId) -> Option<&MembershipStatus> {
        self.statuses.get(operation_id)
    }
}
