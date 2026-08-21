//! Deterministic projection of signed, causal workspace membership operations.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};

use iroh::{PublicKey, Signature};
use serde::{Deserialize, Serialize};

use crate::{
    identity::{InstallationIdentity, PublicIdentity},
    workspace_domain::Member,
};

const MEMBERSHIP_OPERATION_DOMAIN: &[u8] = b"resonance.membership-op.v1\0";
pub const MEMBERSHIP_PROTOCOL_VERSION: u8 = 1;

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
    pub fn contains(&self, public_identity: &str) -> bool {
        self.members
            .iter()
            .any(|member| member.public_identity == public_identity)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MembershipError {
    Encode,
    Decode,
    InvalidOperationId,
    InvalidSignature,
}

impl fmt::Display for MembershipError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode => formatter.write_str("membership operation could not be encoded"),
            Self::Decode => formatter.write_str("membership operation could not be decoded"),
            Self::InvalidOperationId => formatter.write_str("membership operation ID is invalid"),
            Self::InvalidSignature => {
                formatter.write_str("membership operation signature is invalid")
            }
        }
    }
}

impl std::error::Error for MembershipError {}

#[derive(Default)]
pub struct MembershipLog {
    operations: BTreeMap<String, SignedMembershipOperation>,
}

impl MembershipLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_bytes(&mut self, bytes: &[u8]) -> Result<String, MembershipError> {
        let operation = postcard::from_bytes(bytes).map_err(|_| MembershipError::Decode)?;
        self.insert(operation)
    }

    pub fn insert(
        &mut self,
        operation: SignedMembershipOperation,
    ) -> Result<String, MembershipError> {
        let operation_id = operation.operation_id()?;
        self.operations
            .entry(operation_id.clone())
            .or_insert(operation);
        Ok(operation_id)
    }

    #[must_use]
    pub fn next_author_counter(&self, author: &[u8; 32]) -> u64 {
        self.operations
            .values()
            .filter(|operation| &operation.operation.author == author)
            .map(|operation| operation.operation.author_counter)
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }

    #[must_use]
    pub fn encoded_operations(&self) -> Vec<Vec<u8>> {
        self.operations
            .values()
            .filter_map(|operation| operation.encode().ok())
            .collect()
    }

    #[must_use]
    pub fn projection(&self, workspace_id: &str) -> MembershipProjection {
        let mut statuses = self
            .operations
            .keys()
            .map(|id| (id.clone(), MembershipStatus::Rejected))
            .collect::<BTreeMap<_, _>>();
        let mut members = BTreeMap::new();
        let mut counters = BTreeMap::new();

        let mut genesis = self
            .operations
            .iter()
            .filter(|(_, signed)| valid_genesis(signed, workspace_id))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        genesis.sort();
        let Some(mut head) = genesis.into_iter().next() else {
            mark_pending_operations(&self.operations, workspace_id, &mut statuses);
            return MembershipProjection {
                canonical_head: None,
                members: Vec::new(),
                statuses,
            };
        };

        let mut canonical = BTreeSet::new();
        let first = &self.operations[&head];
        apply_addition(&mut members, &mut counters, first);
        canonical.insert(head.clone());
        statuses.insert(head.clone(), MembershipStatus::Canonical);

        loop {
            let candidates = self
                .operations
                .iter()
                .filter(|(_, signed)| {
                    signed.operation.parent_operation_id.as_deref() == Some(&head)
                })
                .filter(|(_, signed)| valid_child(signed, workspace_id, &members, &counters))
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>();
            let Some(next) = candidates.into_iter().next() else {
                break;
            };
            let operation = &self.operations[&next];
            apply_addition(&mut members, &mut counters, operation);
            statuses.insert(next.clone(), MembershipStatus::Canonical);
            canonical.insert(next.clone());
            head = next;
        }

        mark_pending_operations(&self.operations, workspace_id, &mut statuses);
        for id in canonical {
            statuses.insert(id, MembershipStatus::Canonical);
        }
        MembershipProjection {
            canonical_head: Some(head),
            members: members.into_values().collect(),
            statuses,
        }
    }
}

impl SignedMembershipOperation {
    pub fn genesis(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        display_name: impl Into<String>,
        added_at: i64,
    ) -> Result<Self, MembershipError> {
        let public_identity = *identity.public_identity().as_bytes();
        Self::sign(
            identity,
            MembershipOperation {
                version: MEMBERSHIP_PROTOCOL_VERSION,
                workspace_id: workspace_id.into(),
                parent_operation_id: None,
                author: public_identity,
                author_counter: 0,
                body: MembershipOperationBody::AddMember {
                    public_identity,
                    display_name: display_name.into(),
                    role: "developer".to_owned(),
                    added_at,
                },
            },
        )
    }

    pub fn add_member(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        parent_operation_id: impl Into<String>,
        author_counter: u64,
        public_identity: [u8; 32],
        display_name: impl Into<String>,
        added_at: i64,
    ) -> Result<Self, MembershipError> {
        Self::sign(
            identity,
            MembershipOperation {
                version: MEMBERSHIP_PROTOCOL_VERSION,
                workspace_id: workspace_id.into(),
                parent_operation_id: Some(parent_operation_id.into()),
                author: *identity.public_identity().as_bytes(),
                author_counter,
                body: MembershipOperationBody::AddMember {
                    public_identity,
                    display_name: display_name.into(),
                    role: "contributor".to_owned(),
                    added_at,
                },
            },
        )
    }

    pub fn encode(&self) -> Result<Vec<u8>, MembershipError> {
        postcard::to_stdvec(self).map_err(|_| MembershipError::Encode)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MembershipError> {
        postcard::from_bytes(bytes).map_err(|_| MembershipError::Decode)
    }

    pub fn operation_id(&self) -> Result<String, MembershipError> {
        let encoded = self.encode()?;
        Ok(blake3::hash(&encoded).to_hex().to_string())
    }

    pub fn verify(&self) -> Result<(), MembershipError> {
        let signer = PublicKey::from_bytes(&self.operation.author)
            .map_err(|_| MembershipError::InvalidSignature)?;
        let signature: [u8; Signature::LENGTH] = self
            .signature
            .as_slice()
            .try_into()
            .map_err(|_| MembershipError::InvalidSignature)?;
        signer
            .verify(
                &membership_signing_bytes(&self.operation)?,
                &Signature::from_bytes(&signature),
            )
            .map_err(|_| MembershipError::InvalidSignature)
    }

    fn sign(
        identity: &InstallationIdentity,
        operation: MembershipOperation,
    ) -> Result<Self, MembershipError> {
        let signature = identity.sign(&membership_signing_bytes(&operation)?);
        Ok(Self {
            operation,
            signature: signature.to_vec(),
        })
    }
}

fn membership_signing_bytes(operation: &MembershipOperation) -> Result<Vec<u8>, MembershipError> {
    let mut bytes = MEMBERSHIP_OPERATION_DOMAIN.to_vec();
    bytes.extend(postcard::to_stdvec(operation).map_err(|_| MembershipError::Encode)?);
    Ok(bytes)
}

fn valid_genesis(operation: &SignedMembershipOperation, workspace_id: &str) -> bool {
    let MembershipOperationBody::AddMember {
        public_identity,
        role,
        display_name,
        ..
    } = &operation.operation.body;
    operation.operation.version == MEMBERSHIP_PROTOCOL_VERSION
        && operation.operation.workspace_id == workspace_id
        && operation.operation.parent_operation_id.is_none()
        && operation.operation.author == *public_identity
        && operation.operation.author_counter == 0
        && role == "developer"
        && valid_member_fields(display_name, role)
        && operation.verify().is_ok()
}

fn valid_child(
    operation: &SignedMembershipOperation,
    workspace_id: &str,
    members: &BTreeMap<String, Member>,
    counters: &BTreeMap<String, u64>,
) -> bool {
    let MembershipOperationBody::AddMember {
        public_identity,
        display_name,
        role,
        ..
    } = &operation.operation.body;
    let author = public_identity_text(&operation.operation.author);
    let added = public_identity_text(public_identity);
    operation.operation.version == MEMBERSHIP_PROTOCOL_VERSION
        && operation.operation.workspace_id == workspace_id
        && operation
            .operation
            .parent_operation_id
            .as_deref()
            .is_some_and(valid_operation_id)
        && members.contains_key(&author)
        && operation.operation.author_counter > counters.get(&author).copied().unwrap_or(0)
        && !members.contains_key(&added)
        && valid_member_fields(display_name, role)
        && operation.verify().is_ok()
}

fn valid_member_fields(display_name: &str, role: &str) -> bool {
    !display_name.trim().is_empty()
        && display_name.len() <= 256
        && !role.trim().is_empty()
        && role.len() <= 64
}

fn apply_addition(
    members: &mut BTreeMap<String, Member>,
    counters: &mut BTreeMap<String, u64>,
    operation: &SignedMembershipOperation,
) {
    let MembershipOperationBody::AddMember {
        public_identity,
        display_name,
        role,
        added_at,
    } = &operation.operation.body;
    let author = public_identity_text(&operation.operation.author);
    members.insert(
        public_identity_text(public_identity),
        Member::new(
            public_identity_text(public_identity),
            display_name,
            role,
            &author,
            *added_at,
        ),
    );
    counters.insert(author, operation.operation.author_counter);
}

fn mark_pending_operations(
    operations: &BTreeMap<String, SignedMembershipOperation>,
    workspace_id: &str,
    statuses: &mut BTreeMap<String, MembershipStatus>,
) {
    for (id, operation) in operations {
        let parent = operation.operation.parent_operation_id.as_deref();
        if operation.operation.version == MEMBERSHIP_PROTOCOL_VERSION
            && operation.operation.workspace_id == workspace_id
            && operation.verify().is_ok()
            && parent.is_some_and(valid_operation_id)
            && !operations.contains_key(parent.expect("parent was checked"))
        {
            statuses.insert(id.clone(), MembershipStatus::Pending);
        }
    }
}

fn valid_operation_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn public_identity_text(public_identity: &[u8; 32]) -> String {
    PublicIdentity::from_bytes(*public_identity).to_string()
}

#[cfg(test)]
mod tests;
