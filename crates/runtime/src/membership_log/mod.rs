//! Deterministic projection of signed, causal workspace membership operations.

use std::{collections::BTreeMap, fmt};

use crate::{
    identity::{InstallationIdentity, PublicIdentity},
    workspace_domain::Member,
};
use iroh::{PublicKey, Signature};

const MEMBERSHIP_OPERATION_DOMAIN: &[u8] = b"resonance.membership-op.v1\0";
const SELF_REMOVAL_REQUEST_DOMAIN: &[u8] = b"resonance.self-removal-request.v1\0";
pub const MEMBERSHIP_PROTOCOL_VERSION: u8 = 1;
pub const SELF_REMOVAL_REQUEST_VERSION: u8 = 1;

mod operation;
pub use operation::{
    MembershipOperation, MembershipOperationBody, RemovalAuthorizationV1, SelfRemovalRequestV1,
    SignedMembershipOperation, SignedSelfRemovalRequestV1,
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MembershipOperationId(String);

impl MembershipOperationId {
    pub fn parse(value: &str) -> Result<Self, MembershipError> {
        if valid_operation_id(value) {
            Ok(Self(value.to_owned()))
        } else {
            Err(MembershipError::InvalidOperationId)
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for MembershipOperationId {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for MembershipOperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

mod projection;
pub use projection::{
    CanonicalRemoval, MembershipInterval, MembershipProjection, MembershipStatus,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedMembershipTransition {
    pub operation: SignedMembershipOperation,
    pub operation_id: MembershipOperationId,
    pub exact_operation: Vec<u8>,
    pub before_head: Option<MembershipOperationId>,
    pub resulting_head: MembershipOperationId,
    pub creator: PublicIdentity,
    pub author: PublicIdentity,
    pub additions: Vec<MembershipInterval>,
    pub removals: Vec<MembershipInterval>,
    pub resulting_members: Vec<Member>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum MembershipError {
    Encode,
    Decode,
    InvalidOperationId,
    InvalidSignature,
    InvalidRequest,
    UnauthorizedTransition(&'static str),
    RandomnessUnavailable,
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
            Self::InvalidRequest => formatter.write_str("self-removal request is invalid"),
            Self::UnauthorizedTransition(reason) => {
                write!(formatter, "membership transition is unauthorized: {reason}")
            }
            Self::RandomnessUnavailable => {
                formatter.write_str("membership request randomness is unavailable")
            }
        }
    }
}

impl std::error::Error for MembershipError {}

#[derive(Clone, Default)]
pub struct MembershipLog {
    operations: BTreeMap<MembershipOperationId, SignedMembershipOperation>,
}

impl MembershipLog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_bytes(&mut self, bytes: &[u8]) -> Result<String, MembershipError> {
        let operation = SignedMembershipOperation::decode(bytes)?;
        self.insert(operation)
    }

    pub fn insert(
        &mut self,
        operation: SignedMembershipOperation,
    ) -> Result<String, MembershipError> {
        let operation_id = operation.operation_id_value()?;
        self.operations
            .entry(operation_id.clone())
            .or_insert(operation);
        Ok(operation_id.to_string())
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

    pub fn encoded_operations(&self) -> Result<Vec<Vec<u8>>, MembershipError> {
        self.operations
            .values()
            .map(SignedMembershipOperation::encode)
            .collect()
    }

    pub fn prepare(
        &self,
        workspace_id: &str,
        operation: SignedMembershipOperation,
    ) -> Result<PreparedMembershipTransition, MembershipError> {
        let before = self.projection(workspace_id);
        let before_head = before.canonical_head.clone();
        if operation.operation.parent_operation_id.as_deref()
            != before_head.as_ref().map(MembershipOperationId::as_str)
        {
            return Err(MembershipError::UnauthorizedTransition(
                "operation does not extend the current canonical head",
            ));
        }
        let operation_id = operation.operation_id_value()?;
        let exact_operation = operation.encode()?;
        let mut staged = self.clone();
        staged.insert(operation.clone())?;
        let after = staged.projection(workspace_id);
        if after.canonical_head.as_ref() != Some(&operation_id) {
            return Err(MembershipError::UnauthorizedTransition(
                "operation does not produce the canonical membership result",
            ));
        }
        let creator = after
            .creator
            .ok_or(MembershipError::UnauthorizedTransition(
                "canonical genesis creator is unavailable",
            ))?;
        let additions = after
            .intervals
            .iter()
            .filter(|(identity, interval)| {
                before.interval_id(identity) != Some(&interval.interval_id)
            })
            .map(|(_, interval)| interval.clone())
            .collect();
        let removals = before
            .intervals
            .iter()
            .filter(|(identity, _)| !after.intervals.contains_key(identity))
            .map(|(_, interval)| interval.clone())
            .collect();
        Ok(PreparedMembershipTransition {
            operation,
            operation_id: operation_id.clone(),
            exact_operation,
            before_head,
            resulting_head: operation_id,
            creator,
            author: PublicIdentity::from_bytes(after_operation_author(&staged, workspace_id)?),
            additions,
            removals,
            resulting_members: after.members,
        })
    }

    pub fn projection_at(
        &self,
        workspace_id: &str,
        head: &MembershipOperationId,
    ) -> Option<MembershipProjection> {
        let current = self.projection(workspace_id);
        let position = current
            .canonical_lineage
            .iter()
            .position(|candidate| candidate == head)?;
        let mut historical = Self::new();
        for operation_id in &current.canonical_lineage[..=position] {
            historical.operations.insert(
                operation_id.clone(),
                self.operations.get(operation_id)?.clone(),
            );
        }
        Some(historical.projection(workspace_id))
    }

    pub fn operation_author(&self, operation_id: &MembershipOperationId) -> Option<PublicIdentity> {
        self.operations
            .get(operation_id)
            .map(|signed| PublicIdentity::from_bytes(signed.operation.author))
    }

    pub fn current_transition(
        &self,
        workspace_id: &str,
    ) -> Result<PreparedMembershipTransition, MembershipError> {
        let projection = self.projection(workspace_id);
        let resulting_head =
            projection
                .canonical_head
                .clone()
                .ok_or(MembershipError::UnauthorizedTransition(
                    "canonical membership head is unavailable",
                ))?;
        let operation = self.operations.get(&resulting_head).cloned().ok_or(
            MembershipError::UnauthorizedTransition(
                "canonical membership operation is unavailable",
            ),
        )?;
        let before_head = operation
            .operation
            .parent_operation_id
            .as_deref()
            .map(MembershipOperationId::parse)
            .transpose()?;
        let creator = projection
            .creator
            .ok_or(MembershipError::UnauthorizedTransition(
                "canonical genesis creator is unavailable",
            ))?;
        Ok(PreparedMembershipTransition {
            exact_operation: operation.encode()?,
            operation_id: resulting_head.clone(),
            before_head,
            resulting_head,
            author: PublicIdentity::from_bytes(operation.operation.author),
            operation,
            creator,
            additions: projection.intervals.values().cloned().collect(),
            removals: Vec::new(),
            resulting_members: projection.members,
        })
    }

    #[must_use]
    pub fn projection(&self, workspace_id: &str) -> MembershipProjection {
        let mut statuses = self
            .operations
            .keys()
            .cloned()
            .map(|id| (id, MembershipStatus::Rejected))
            .collect::<BTreeMap<_, _>>();
        let mut intervals = BTreeMap::<PublicIdentity, MembershipInterval>::new();
        let mut latest_removals = BTreeMap::<PublicIdentity, CanonicalRemoval>::new();
        let mut counters = BTreeMap::<PublicIdentity, u64>::new();

        let Some((mut head, genesis)) = self
            .operations
            .iter()
            .find(|(_, operation)| valid_genesis(operation, workspace_id))
        else {
            mark_pending_operations(&self.operations, workspace_id, &mut statuses);
            return MembershipProjection {
                canonical_head: None,
                creator: None,
                members: Vec::new(),
                intervals,
                latest_removals,
                canonical_lineage: Vec::new(),
                statuses,
            };
        };
        let creator = PublicIdentity::from_bytes(genesis.operation.author);
        apply_operation(
            head,
            genesis,
            &mut intervals,
            &mut latest_removals,
            &mut counters,
        );
        statuses.insert(head.clone(), MembershipStatus::Canonical);
        let mut lineage = vec![head.clone()];

        loop {
            let next = self.operations.iter().find(|(_, operation)| {
                operation.operation.parent_operation_id.as_deref() == Some(head.as_str())
                    && valid_child(
                        operation,
                        workspace_id,
                        creator,
                        &intervals,
                        &latest_removals,
                        &counters,
                    )
            });
            let Some((next_id, operation)) = next else {
                break;
            };
            apply_operation(
                next_id,
                operation,
                &mut intervals,
                &mut latest_removals,
                &mut counters,
            );
            statuses.insert(next_id.clone(), MembershipStatus::Canonical);
            lineage.push(next_id.clone());
            head = next_id;
        }

        mark_pending_operations(&self.operations, workspace_id, &mut statuses);
        for id in &lineage {
            statuses.insert(id.clone(), MembershipStatus::Canonical);
        }
        MembershipProjection {
            canonical_head: Some(head.clone()),
            creator: Some(creator),
            members: intervals
                .values()
                .map(|interval| interval.member.clone())
                .collect(),
            intervals,
            latest_removals,
            canonical_lineage: lineage,
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

    #[allow(clippy::too_many_arguments)]
    pub fn add_member(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        parent_operation_id: impl Into<String>,
        author_counter: u64,
        public_identity: [u8; 32],
        display_name: impl Into<String>,
        added_at: i64,
    ) -> Result<Self, MembershipError> {
        Self::add_member_with_role(
            identity,
            workspace_id,
            parent_operation_id,
            author_counter,
            public_identity,
            display_name,
            "contributor",
            added_at,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_member_with_role(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        parent_operation_id: impl Into<String>,
        author_counter: u64,
        public_identity: [u8; 32],
        display_name: impl Into<String>,
        role: impl Into<String>,
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
                    role: role.into(),
                    added_at,
                },
            },
        )
    }

    pub fn expel_member(
        creator: &InstallationIdentity,
        workspace_id: impl Into<String>,
        parent_operation_id: impl Into<String>,
        author_counter: u64,
        public_identity: [u8; 32],
        membership_interval_id: impl Into<String>,
        removed_at: i64,
    ) -> Result<Self, MembershipError> {
        Self::remove_member(
            creator,
            workspace_id,
            parent_operation_id,
            author_counter,
            public_identity,
            membership_interval_id,
            removed_at,
            RemovalAuthorizationV1::CreatorExpulsion,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn remove_member_by_request(
        creator: &InstallationIdentity,
        workspace_id: impl Into<String>,
        parent_operation_id: impl Into<String>,
        author_counter: u64,
        request: SignedSelfRemovalRequestV1,
        removed_at: i64,
    ) -> Result<Self, MembershipError> {
        let public_identity = request.request.requester;
        let interval = request.request.membership_interval_id.clone();
        Self::remove_member(
            creator,
            workspace_id,
            parent_operation_id,
            author_counter,
            public_identity,
            interval,
            removed_at,
            RemovalAuthorizationV1::MemberRequest(request),
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn remove_member(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        parent_operation_id: impl Into<String>,
        author_counter: u64,
        public_identity: [u8; 32],
        membership_interval_id: impl Into<String>,
        removed_at: i64,
        authorization: RemovalAuthorizationV1,
    ) -> Result<Self, MembershipError> {
        Self::sign(
            identity,
            MembershipOperation {
                version: MEMBERSHIP_PROTOCOL_VERSION,
                workspace_id: workspace_id.into(),
                parent_operation_id: Some(parent_operation_id.into()),
                author: *identity.public_identity().as_bytes(),
                author_counter,
                body: MembershipOperationBody::RemoveMember {
                    public_identity,
                    membership_interval_id: membership_interval_id.into(),
                    removed_at,
                    authorization,
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
        Ok(self.operation_id_value()?.to_string())
    }

    pub fn operation_id_value(&self) -> Result<MembershipOperationId, MembershipError> {
        let encoded = self.encode()?;
        MembershipOperationId::parse(blake3::hash(&encoded).to_hex().as_ref())
    }

    pub fn verify(&self) -> Result<(), MembershipError> {
        verify_iroh_signature(
            self.operation.author,
            &membership_signing_bytes(&self.operation)?,
            &self.signature,
        )
    }

    pub(crate) fn sign(
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

impl SignedSelfRemovalRequestV1 {
    pub fn create(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        membership_interval_id: impl Into<String>,
        genesis_creator: [u8; 32],
        requested_at: i64,
    ) -> Result<Self, MembershipError> {
        let mut nonce = [0; 32];
        getrandom::fill(&mut nonce).map_err(|_| MembershipError::RandomnessUnavailable)?;
        Self::create_with_nonce(
            identity,
            workspace_id,
            membership_interval_id,
            genesis_creator,
            nonce,
            requested_at,
        )
    }

    #[doc(hidden)]
    pub fn create_with_nonce(
        identity: &InstallationIdentity,
        workspace_id: impl Into<String>,
        membership_interval_id: impl Into<String>,
        genesis_creator: [u8; 32],
        nonce: [u8; 32],
        requested_at: i64,
    ) -> Result<Self, MembershipError> {
        let request = SelfRemovalRequestV1 {
            version: SELF_REMOVAL_REQUEST_VERSION,
            workspace_id: workspace_id.into(),
            requester: *identity.public_identity().as_bytes(),
            membership_interval_id: membership_interval_id.into(),
            genesis_creator,
            nonce,
            requested_at,
        };
        validate_request_fields(&request)?;
        let signature = identity.sign(&self_removal_signing_bytes(&request)?);
        Ok(Self {
            request,
            signature: signature.to_vec(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>, MembershipError> {
        postcard::to_stdvec(self).map_err(|_| MembershipError::Encode)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, MembershipError> {
        postcard::from_bytes(bytes).map_err(|_| MembershipError::Decode)
    }

    pub fn request_id(&self) -> Result<MembershipOperationId, MembershipError> {
        MembershipOperationId::parse(blake3::hash(&self.encode()?).to_hex().as_ref())
    }

    pub fn verify(&self) -> Result<(), MembershipError> {
        validate_request_fields(&self.request)?;
        verify_iroh_signature(
            self.request.requester,
            &self_removal_signing_bytes(&self.request)?,
            &self.signature,
        )
        .map_err(|_| MembershipError::InvalidRequest)
    }
}

fn valid_genesis(operation: &SignedMembershipOperation, workspace_id: &str) -> bool {
    let MembershipOperationBody::AddMember {
        public_identity,
        role,
        display_name,
        ..
    } = &operation.operation.body
    else {
        return false;
    };
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
    creator: PublicIdentity,
    intervals: &BTreeMap<PublicIdentity, MembershipInterval>,
    removals: &BTreeMap<PublicIdentity, CanonicalRemoval>,
    counters: &BTreeMap<PublicIdentity, u64>,
) -> bool {
    let author = PublicIdentity::from_bytes(operation.operation.author);
    if operation.operation.version != MEMBERSHIP_PROTOCOL_VERSION
        || operation.operation.workspace_id != workspace_id
        || !intervals.contains_key(&author)
        || operation.operation.author_counter <= counters.get(&author).copied().unwrap_or(0)
        || operation.verify().is_err()
    {
        return false;
    }
    match &operation.operation.body {
        MembershipOperationBody::AddMember {
            public_identity,
            display_name,
            role,
            ..
        } => {
            let added = PublicIdentity::from_bytes(*public_identity);
            if intervals.contains_key(&added) || !valid_member_fields(display_name, role) {
                return false;
            }
            match removals.get(&added).map(|removal| &removal.authorization) {
                None | Some(RemovalAuthorizationV1::MemberRequest(_)) => true,
                Some(RemovalAuthorizationV1::CreatorExpulsion) => author == creator,
            }
        }
        MembershipOperationBody::RemoveMember {
            public_identity,
            membership_interval_id,
            authorization,
            ..
        } => {
            let target = PublicIdentity::from_bytes(*public_identity);
            if target == creator
                || intervals
                    .get(&target)
                    .map(|current| current.interval_id.as_str())
                    != Some(membership_interval_id.as_str())
                || author != creator
            {
                return false;
            }
            match authorization {
                RemovalAuthorizationV1::CreatorExpulsion => true,
                RemovalAuthorizationV1::MemberRequest(request) => {
                    request.verify().is_ok()
                        && request.request.workspace_id == workspace_id
                        && request.request.requester == *public_identity
                        && request.request.membership_interval_id == *membership_interval_id
                        && request.request.genesis_creator == *creator.as_bytes()
                }
            }
        }
    }
}

fn apply_operation(
    operation_id: &MembershipOperationId,
    operation: &SignedMembershipOperation,
    intervals: &mut BTreeMap<PublicIdentity, MembershipInterval>,
    removals: &mut BTreeMap<PublicIdentity, CanonicalRemoval>,
    counters: &mut BTreeMap<PublicIdentity, u64>,
) {
    let author = PublicIdentity::from_bytes(operation.operation.author);
    match &operation.operation.body {
        MembershipOperationBody::AddMember {
            public_identity,
            display_name,
            role,
            added_at,
        } => {
            let identity = PublicIdentity::from_bytes(*public_identity);
            intervals.insert(
                identity,
                MembershipInterval {
                    interval_id: operation_id.clone(),
                    member: Member::new(identity, display_name, role, author, *added_at),
                },
            );
        }
        MembershipOperationBody::RemoveMember {
            public_identity,
            membership_interval_id,
            authorization,
            ..
        } => {
            let identity = PublicIdentity::from_bytes(*public_identity);
            intervals.remove(&identity);
            removals.insert(
                identity,
                CanonicalRemoval {
                    interval_id: MembershipOperationId::parse(membership_interval_id)
                        .expect("validated interval ID remains valid"),
                    authorization: authorization.clone(),
                },
            );
        }
    }
    counters.insert(author, operation.operation.author_counter);
}

fn after_operation_author(
    staged: &MembershipLog,
    workspace_id: &str,
) -> Result<[u8; 32], MembershipError> {
    let head = staged.projection(workspace_id).canonical_head.ok_or(
        MembershipError::UnauthorizedTransition("missing resulting head"),
    )?;
    Ok(staged.operations[&head].operation.author)
}

fn membership_signing_bytes(operation: &MembershipOperation) -> Result<Vec<u8>, MembershipError> {
    let mut bytes = MEMBERSHIP_OPERATION_DOMAIN.to_vec();
    bytes.extend(postcard::to_stdvec(operation).map_err(|_| MembershipError::Encode)?);
    Ok(bytes)
}

fn self_removal_signing_bytes(request: &SelfRemovalRequestV1) -> Result<Vec<u8>, MembershipError> {
    let mut bytes = SELF_REMOVAL_REQUEST_DOMAIN.to_vec();
    bytes.extend(postcard::to_stdvec(request).map_err(|_| MembershipError::Encode)?);
    Ok(bytes)
}

fn verify_iroh_signature(
    signer: [u8; 32],
    bytes: &[u8],
    signature: &[u8],
) -> Result<(), MembershipError> {
    let signer = PublicKey::from_bytes(&signer).map_err(|_| MembershipError::InvalidSignature)?;
    let signature: [u8; Signature::LENGTH] = signature
        .try_into()
        .map_err(|_| MembershipError::InvalidSignature)?;
    signer
        .verify(bytes, &Signature::from_bytes(&signature))
        .map_err(|_| MembershipError::InvalidSignature)
}

fn validate_request_fields(request: &SelfRemovalRequestV1) -> Result<(), MembershipError> {
    if request.version != SELF_REMOVAL_REQUEST_VERSION
        || !valid_operation_id(&request.workspace_id)
        || !valid_operation_id(&request.membership_interval_id)
        || request.requester == request.genesis_creator
    {
        return Err(MembershipError::InvalidRequest);
    }
    Ok(())
}

fn valid_member_fields(display_name: &str, role: &str) -> bool {
    !display_name.trim().is_empty()
        && display_name.len() <= 256
        && !role.trim().is_empty()
        && role.len() <= 64
}

fn mark_pending_operations(
    operations: &BTreeMap<MembershipOperationId, SignedMembershipOperation>,
    workspace_id: &str,
    statuses: &mut BTreeMap<MembershipOperationId, MembershipStatus>,
) {
    for (id, operation) in operations {
        let parent = operation.operation.parent_operation_id.as_deref();
        if operation.operation.version == MEMBERSHIP_PROTOCOL_VERSION
            && operation.operation.workspace_id == workspace_id
            && operation.verify().is_ok()
            && parent.is_some_and(valid_operation_id)
            && MembershipOperationId::parse(parent.expect("parent was checked"))
                .map(|parent| !operations.contains_key(&parent))
                .unwrap_or(false)
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

#[cfg(test)]
mod tests;
