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
    /// Compatibility text view of the v1 operation identifiers.
    pub canonical_head: Option<String>,
    /// Typed operation identity used by runtime consumers.
    pub canonical_head_id: Option<MembershipOperationId>,
    pub members: Vec<Member>,
    /// Compatibility text view of the v1 operation identifiers.
    pub statuses: BTreeMap<String, MembershipStatus>,
    /// Typed status map used by runtime consumers and storage seams.
    pub statuses_by_id: BTreeMap<MembershipOperationId, MembershipStatus>,
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
        self.statuses_by_id.get(operation_id)
    }
}
