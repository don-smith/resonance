use std::collections::BTreeMap;

use crate::identity::PublicIdentity;
use crate::workspace_domain::Member;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MembershipStatus {
    Canonical,
    Pending,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MembershipProjection {
    pub canonical_head: Option<String>,
    pub members: Vec<Member>,
    pub statuses: BTreeMap<String, MembershipStatus>,
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
}
