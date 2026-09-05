//! Membership-bound lookup peer sets and validated address replacement.

use std::{collections::BTreeMap, net::SocketAddr};

use crate::{
    identity::{InstallationIdentity, PublicIdentity},
    membership_log::MembershipLog,
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

use super::{
    address_directory::{AddressDirectory, DirectPeerRoute},
    channels::membership_id,
    mesh::{ConversationMeshError, ProductionConversationMesh},
    wire::{AddressNoticeV1, ConversationRecordV1, ExactRecordV1},
    ConversationError,
};

#[derive(Debug)]
pub enum ConversationLookupError {
    Conversation(ConversationError),
    Mesh(ConversationMeshError),
    Store(WorkspaceStoreError),
    MissingMembershipHead,
}

impl std::fmt::Display for ConversationLookupError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conversation(error) => {
                write!(formatter, "conversation lookup rejected control: {error}")
            }
            Self::Mesh(error) => write!(formatter, "conversation lookup failed: {error}"),
            Self::Store(error) => write!(formatter, "conversation lookup storage failed: {error}"),
            Self::MissingMembershipHead => {
                formatter.write_str("conversation lookup needs a canonical membership head")
            }
        }
    }
}

impl std::error::Error for ConversationLookupError {}

impl From<ConversationError> for ConversationLookupError {
    fn from(error: ConversationError) -> Self {
        Self::Conversation(error)
    }
}

impl From<WorkspaceStoreError> for ConversationLookupError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Store(error)
    }
}

impl From<ConversationMeshError> for ConversationLookupError {
    fn from(error: ConversationMeshError) -> Self {
        Self::Mesh(error)
    }
}

pub struct ConversationLookup {
    identity: InstallationIdentity,
    workspace_id: String,
    workspace_bytes: [u8; 32],
    membership: MembershipLog,
    directory: AddressDirectory,
    mesh: ProductionConversationMesh,
    tracked_version: Option<u64>,
    store: Option<WorkspaceStore>,
    authenticated_observations: BTreeMap<PublicIdentity, i64>,
    last_candidate_rotation: i64,
}

impl ConversationLookup {
    pub fn start(
        identity: InstallationIdentity,
        workspace_id: impl Into<String>,
        membership: MembershipLog,
        listen: SocketAddr,
    ) -> Result<Self, ConversationLookupError> {
        Self::start_internal(identity, workspace_id.into(), membership, listen, None, 0)
    }

    pub fn start_with_store(
        identity: InstallationIdentity,
        workspace_id: impl Into<String>,
        membership: MembershipLog,
        listen: SocketAddr,
        store: WorkspaceStore,
        now: i64,
    ) -> Result<Self, ConversationLookupError> {
        Self::start_internal(
            identity,
            workspace_id.into(),
            membership,
            listen,
            Some(store),
            now,
        )
    }

    fn start_internal(
        identity: InstallationIdentity,
        workspace_id: String,
        membership: MembershipLog,
        listen: SocketAddr,
        store: Option<WorkspaceStore>,
        now: i64,
    ) -> Result<Self, ConversationLookupError> {
        let workspace_bytes = decode_hex_32(&workspace_id)
            .ok_or(ConversationError::MalformedBytes("workspace ID"))?;
        let mesh = ProductionConversationMesh::start(identity.clone(), workspace_bytes, listen)?;
        let mut directory = AddressDirectory::default();
        if let Some(store) = store.as_ref() {
            for bytes in store.conversation_address_notices()? {
                let exact = ExactRecordV1::decode(&bytes)?;
                let ConversationRecordV1::AddressNotice(record) = exact.record() else {
                    return Err(ConversationError::UnauthorizedData(
                        "persisted address control has another record family",
                    )
                    .into());
                };
                let projection = membership.projection(&workspace_id);
                if record.expires_at > now
                    && projection.contains(&PublicIdentity::from_bytes(record.sender))
                    && membership_id(&record.observed_membership_head).as_ref()
                        == projection.canonical_head.as_ref()
                {
                    directory.accept(&bytes, &membership, &workspace_id, now)?;
                }
            }
        }
        Ok(Self {
            identity,
            workspace_id,
            workspace_bytes,
            membership,
            directory,
            mesh,
            tracked_version: None,
            store,
            authenticated_observations: BTreeMap::new(),
            last_candidate_rotation: now,
        })
    }

    pub fn local_address_notice(
        &self,
        generation: u64,
        expires_at: i64,
    ) -> Result<ExactRecordV1, ConversationLookupError> {
        self.local_address_notice_with_addresses(
            generation,
            expires_at,
            vec![self.mesh.listen_addr()],
        )
    }

    pub fn local_address_notice_with_addresses(
        &self,
        generation: u64,
        expires_at: i64,
        addresses: Vec<SocketAddr>,
    ) -> Result<ExactRecordV1, ConversationLookupError> {
        let projection = self.membership.projection(&self.workspace_id);
        let head = projection
            .canonical_head
            .as_ref()
            .and_then(|head| decode_hex_32(head.as_str()))
            .ok_or(ConversationLookupError::MissingMembershipHead)?;
        Ok(ExactRecordV1::author(
            ConversationRecordV1::AddressNotice(AddressNoticeV1 {
                workspace_id: self.workspace_bytes,
                sender: *self.identity.public_identity().as_bytes(),
                observed_membership_head: head,
                generation,
                expires_at,
                addresses: addresses
                    .into_iter()
                    .map(|address| address.to_string())
                    .collect(),
            }),
            &self.identity,
        )?)
    }

    pub fn accept_address_notice(
        &mut self,
        authenticated_sender: PublicIdentity,
        bytes: &[u8],
        now: i64,
    ) -> Result<bool, ConversationLookupError> {
        let exact = ExactRecordV1::decode(bytes)?;
        if exact.record().signer() != *authenticated_sender.as_bytes() {
            return Err(ConversationError::UnauthorizedData(
                "address notice sender does not match outer sender",
            )
            .into());
        }
        let mut directory = self.directory.clone();
        let changed = directory.accept(bytes, &self.membership, &self.workspace_id, now)?;
        if changed {
            if let Some(store) = self.store.as_ref() {
                let ConversationRecordV1::AddressNotice(record) = exact.record() else {
                    unreachable!("address family checked by directory");
                };
                store.record_conversation_address_notice(
                    &record.sender,
                    record.generation,
                    record.expires_at,
                    exact.bytes(),
                )?;
            }
            self.directory = directory;
        }
        if changed && self.tracked_version.is_some() {
            let routes = self.routes_with_local(now);
            self.mesh.overwrite_addresses(routes)?;
        }
        Ok(changed)
    }

    pub fn replace_membership(
        &mut self,
        membership: MembershipLog,
        durable_peer_set_version: u64,
        now: i64,
    ) -> Result<(), ConversationLookupError> {
        self.membership = membership;
        if self.tracked_version != Some(durable_peer_set_version) {
            let routes = self.routes_with_local(now);
            self.mesh.track_members(durable_peer_set_version, routes)?;
            self.tracked_version = Some(durable_peer_set_version);
        }
        Ok(())
    }

    #[must_use]
    pub fn another_member_has_candidate(&mut self, now: i64) -> bool {
        self.directory.has_candidate_for_other_member(
            &self.membership,
            &self.workspace_id,
            self.identity.public_identity(),
            now,
        )
    }

    pub fn rotate_direct_candidates(&mut self, now: i64) -> Result<(), ConversationLookupError> {
        self.directory.rotate_candidates();
        let routes = self.routes_with_local(now);
        self.mesh.overwrite_addresses(routes)?;
        self.last_candidate_rotation = now;
        Ok(())
    }

    pub fn record_authenticated_observation(&mut self, peer: PublicIdentity, now: i64) {
        if self
            .membership
            .projection(&self.workspace_id)
            .contains(&peer)
        {
            self.authenticated_observations.insert(peer, now);
        }
    }

    pub fn maintain_direct_candidates(&mut self, now: i64) -> Result<(), ConversationLookupError> {
        let current_members = self.membership.projection(&self.workspace_id);
        self.authenticated_observations
            .retain(|identity, observed_at| {
                current_members.contains(identity) && now.saturating_sub(*observed_at) <= 30
            });
        if self.authenticated_observations.is_empty()
            && now.saturating_sub(self.last_candidate_rotation) >= 2
        {
            self.rotate_direct_candidates(now)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn has_recent_usable_peer(&self, now: i64) -> bool {
        self.authenticated_observations
            .values()
            .any(|observed_at| now.saturating_sub(*observed_at) <= 30)
    }

    pub fn candidate_peers(&mut self, now: i64) -> Vec<PublicIdentity> {
        self.directory
            .routes(&self.membership, &self.workspace_id, now)
            .into_iter()
            .filter_map(|route| {
                (route.identity != self.identity.public_identity()).then_some(route.identity)
            })
            .collect()
    }

    #[must_use]
    pub fn mesh(&self) -> &ProductionConversationMesh {
        &self.mesh
    }

    pub fn stop(&mut self) -> Result<(), ConversationLookupError> {
        Ok(self.mesh.stop()?)
    }

    fn routes_with_local(&mut self, now: i64) -> Vec<DirectPeerRoute> {
        let mut routes = self
            .directory
            .routes(&self.membership, &self.workspace_id, now)
            .into_iter()
            .map(|route| (route.identity, route))
            .collect::<BTreeMap<_, _>>();
        for member in self.membership.projection(&self.workspace_id).members {
            routes
                .entry(member.public_identity)
                .or_insert(DirectPeerRoute {
                    identity: member.public_identity,
                    // Lookup requires an address for every authenticated peer. This closed local
                    // candidate keeps canonical membership tracked until a validated notice
                    // overwrites it; the identity handshake prevents accidental authority.
                    candidates: vec![SocketAddr::from(([127, 0, 0, 1], 1))],
                });
        }
        routes.insert(
            self.identity.public_identity(),
            DirectPeerRoute {
                identity: self.identity.public_identity(),
                candidates: vec![self.mesh.listen_addr()],
            },
        );
        routes.into_values().collect()
    }
}

fn decode_hex_32(value: &str) -> Option<[u8; 32]> {
    if value.len() != 64 {
        return None;
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
        output[index] = (hex_nibble(pair[0])? << 4) | hex_nibble(pair[1])?;
    }
    Some(output)
}

const fn hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        _ => None,
    }
}
