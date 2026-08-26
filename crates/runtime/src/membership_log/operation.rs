use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembershipOperation {
    pub version: u8,
    pub workspace_id: String,
    pub parent_operation_id: Option<String>,
    pub author: [u8; 32],
    pub author_counter: u64,
    pub body: MembershipOperationBody,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MembershipOperationBody {
    AddMember {
        public_identity: [u8; 32],
        display_name: String,
        role: String,
        added_at: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedMembershipOperation {
    pub operation: MembershipOperation,
    pub signature: Vec<u8>,
}
