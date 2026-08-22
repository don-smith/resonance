//! Active-workspace state machine over a secret-free delivery port.

use std::{
    collections::BTreeMap,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    identity::InstallationIdentity,
    invite::{validate_relay_override, Invite, InviteError},
    membership_log::{
        MembershipError, MembershipLog, MembershipOperationBody, MembershipProjection,
        SignedMembershipOperation,
    },
    protocol::{Envelope, EnvelopeBody, ProtocolError},
    workspace_catalog::{WorkspaceCatalog, WorkspaceCatalogError},
    workspace_domain::{
        KnownPeer, Member, PeerConnection, WorkspaceLifecycle, WorkspaceSummary, WorkspaceToken,
    },
    workspace_store::WorkspaceStoreError,
};

pub const HEARTBEAT_TTL_SECONDS: i64 = 30;

pub trait DeliveryPort {
    fn send(&mut self, message: Vec<u8>);
}

#[derive(Default)]
pub struct FakeDeliveryPort {
    outbound: Vec<Vec<u8>>,
}

impl FakeDeliveryPort {
    #[must_use]
    pub fn take_outbound(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.outbound)
    }
}

impl DeliveryPort for FakeDeliveryPort {
    fn send(&mut self, message: Vec<u8>) {
        self.outbound.push(message);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveWorkspaceView {
    pub workspace: WorkspaceSummary,
    pub local_public_identity: String,
    pub members: Vec<Member>,
    pub peers: Vec<KnownPeer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceTransition {
    WorkspaceChanged(ActiveWorkspaceView),
    MemberJoined(Member),
    PeerPresenceChanged(KnownPeer),
}

#[derive(Debug)]
pub enum WorkspaceSessionError {
    Catalog(WorkspaceCatalogError),
    Store(WorkspaceStoreError),
    Membership(MembershipError),
    Invite(InviteError),
    Protocol(ProtocolError),
    NoActiveWorkspace,
    InvalidInviteAdmission,
    ClockUnavailable,
}

impl fmt::Display for WorkspaceSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "workspace catalog failed: {error}"),
            Self::Store(error) => write!(formatter, "workspace storage failed: {error}"),
            Self::Membership(error) => write!(formatter, "membership processing failed: {error}"),
            Self::Invite(error) => write!(formatter, "invite processing failed: {error}"),
            Self::Protocol(error) => write!(formatter, "workspace protocol failed: {error}"),
            Self::NoActiveWorkspace => formatter.write_str("there is no active workspace"),
            Self::InvalidInviteAdmission => {
                formatter.write_str("join request is not addressed to this canonical inviter")
            }
            Self::ClockUnavailable => formatter.write_str("system clock is unavailable"),
        }
    }
}

impl std::error::Error for WorkspaceSessionError {}

impl From<WorkspaceCatalogError> for WorkspaceSessionError {
    fn from(error: WorkspaceCatalogError) -> Self {
        Self::Catalog(error)
    }
}
impl From<WorkspaceStoreError> for WorkspaceSessionError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Store(error)
    }
}
impl From<MembershipError> for WorkspaceSessionError {
    fn from(error: MembershipError) -> Self {
        Self::Membership(error)
    }
}
impl From<InviteError> for WorkspaceSessionError {
    fn from(error: InviteError) -> Self {
        Self::Invite(error)
    }
}
impl From<ProtocolError> for WorkspaceSessionError {
    fn from(error: ProtocolError) -> Self {
        Self::Protocol(error)
    }
}

pub struct WorkspaceSession<D: DeliveryPort> {
    identity: InstallationIdentity,
    catalog: WorkspaceCatalog,
    delivery: D,
    active: Option<ActiveWorkspace>,
    transitions: Vec<WorkspaceTransition>,
}

struct ActiveWorkspace {
    summary: WorkspaceSummary,
    log: MembershipLog,
    joining_inviter: Option<[u8; 32]>,
    bootstrap: Option<String>,
    peers: BTreeMap<String, PeerState>,
}

#[derive(Clone)]
struct PeerState {
    last_heartbeat: i64,
    online: bool,
    connection: PeerConnection,
}

impl<D: DeliveryPort> WorkspaceSession<D> {
    #[must_use]
    pub fn new(identity: InstallationIdentity, catalog: WorkspaceCatalog, delivery: D) -> Self {
        Self {
            identity,
            catalog,
            delivery,
            active: None,
            transitions: Vec::new(),
        }
    }

    #[must_use]
    pub fn local_public_identity(&self) -> String {
        self.identity.public_identity().to_string()
    }

    #[must_use]
    pub fn has_active_workspace(&self) -> bool {
        self.active.is_some()
    }

    /// Restores the catalog's active workspace, if this installation has one.
    pub fn activate_active_workspace(
        &mut self,
    ) -> Result<Option<ActiveWorkspaceView>, WorkspaceSessionError> {
        let Some(summary) = self.catalog.active_workspace()? else {
            return Ok(None);
        };
        self.activate(summary)?;
        self.view().map(Some)
    }

    pub fn create_workspace(
        &mut self,
        display_name: impl Into<String>,
        relay_override: Option<String>,
    ) -> Result<ActiveWorkspaceView, WorkspaceSessionError> {
        if let Some(relay) = relay_override.as_deref() {
            validate_relay_override(relay)?;
        }
        let token = WorkspaceToken::generate().map_err(WorkspaceCatalogError::Domain)?;
        let summary = self.catalog.create_workspace_with_token(
            token.clone(),
            display_name.into(),
            relay_override,
            WorkspaceLifecycle::Ready,
        )?;
        self.activate(summary)?;
        let workspace_id = self.active()?.summary.id.as_str().to_owned();
        let genesis = SignedMembershipOperation::genesis(
            &self.identity,
            workspace_id,
            self.identity.public_identity().to_string(),
            now()?,
        )?;
        self.persist_operation(genesis.encode()?)?;
        self.view()
    }

    pub fn create_invite(
        &self,
        bootstrap: impl Into<String>,
    ) -> Result<String, WorkspaceSessionError> {
        let active = self.active()?;
        let store = self.catalog.open_workspace(&active.summary.id)?;
        let settings = store.private_settings()?;
        Ok(Invite::create(
            &self.identity,
            &settings.token,
            settings.display_name,
            settings.relay_override,
            bootstrap,
        )?)
    }

    pub fn join_workspace(
        &mut self,
        encoded_invite: &str,
        display_name: impl Into<String>,
    ) -> Result<ActiveWorkspaceView, WorkspaceSessionError> {
        let invite = Invite::decode(encoded_invite)?;
        let workspace_id = invite.workspace_id().to_owned();
        let summary = self.catalog.create_workspace_with_token(
            invite.workspace_token(),
            invite.workspace_name().to_owned(),
            invite.relay_override().map(ToOwned::to_owned),
            WorkspaceLifecycle::Joining,
        )?;
        let store = self.catalog.open_workspace(&summary.id)?;
        store.set_pending_join_admission(invite.inviter(), invite.bootstrap())?;
        self.activate(summary)?;
        self.send_join_request(display_name.into())?;
        debug_assert_eq!(self.active()?.summary.id.as_str(), workspace_id);
        self.view()
    }

    /// Reissues a pending join request. A workspace already admitted by the
    /// inviter is a successful no-op so callers can refresh stale shell state.
    pub fn retry_join(
        &mut self,
        display_name: impl Into<String>,
    ) -> Result<bool, WorkspaceSessionError> {
        if self.active()?.summary.lifecycle != WorkspaceLifecycle::Joining {
            return Ok(false);
        }
        self.send_join_request(display_name.into())?;
        Ok(true)
    }

    pub fn request_membership_sync(&mut self) -> Result<(), WorkspaceSessionError> {
        let workspace_id = self.active()?.summary.id.as_str().to_owned();
        self.send(EnvelopeBody::MembershipSyncRequest, workspace_id)
    }

    pub(crate) fn is_ready(&self) -> Result<bool, WorkspaceSessionError> {
        Ok(self.active()?.summary.lifecycle == WorkspaceLifecycle::Ready)
    }

    /// Queues a heartbeat only after the local identity is an admitted member.
    pub fn send_heartbeat(&mut self) -> Result<bool, WorkspaceSessionError> {
        if !self.is_ready()? {
            return Ok(false);
        }
        let workspace_id = self.active()?.summary.id.as_str().to_owned();
        self.send(EnvelopeBody::Heartbeat { sent_at: now()? }, workspace_id)?;
        Ok(true)
    }

    /// Refines a validated member's current connection without treating a gossip neighbor as a member.
    pub fn observe_connection(
        &mut self,
        public_identity: [u8; 32],
        connection: PeerConnection,
    ) -> Result<(), WorkspaceSessionError> {
        let public_identity = public_identity_text(&public_identity);
        if !self.projection().contains(&public_identity) {
            return Ok(());
        }
        let peer = {
            let active = self.active_mut()?;
            let state = active
                .peers
                .entry(public_identity.clone())
                .or_insert(PeerState {
                    last_heartbeat: 0,
                    online: false,
                    connection: PeerConnection::Unknown,
                });
            state.connection = connection;
            KnownPeer {
                public_identity: public_identity.clone(),
                online: state.online,
                connection: state.connection.clone(),
            }
        };
        self.transitions
            .push(WorkspaceTransition::PeerPresenceChanged(peer));
        Ok(())
    }

    /// Expires old heartbeat observations while retaining known-member connection data.
    pub fn expire_presence(&mut self, at: i64) -> Result<bool, WorkspaceSessionError> {
        let members = self.projection();
        let changed = self
            .active_mut()?
            .peers
            .iter_mut()
            .filter_map(|(public_identity, state)| {
                if members.contains(public_identity)
                    && state.online
                    && state.last_heartbeat.saturating_add(HEARTBEAT_TTL_SECONDS) < at
                {
                    state.online = false;
                    Some(KnownPeer {
                        public_identity: public_identity.clone(),
                        online: false,
                        connection: state.connection.clone(),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let changed_presence = !changed.is_empty();
        self.transitions.extend(
            changed
                .into_iter()
                .map(WorkspaceTransition::PeerPresenceChanged),
        );
        Ok(changed_presence)
    }

    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), WorkspaceSessionError> {
        let envelope = Envelope::decode(bytes)?;
        envelope.verify()?;
        if envelope.workspace_id != self.active()?.summary.id.as_str() {
            return Err(ProtocolError::InvalidWorkspace.into());
        }
        // Gossip may deliver a publisher's signed envelope back to that same
        // publisher. Local state was already changed before the broadcast.
        if envelope.sender == *self.identity.public_identity().as_bytes() {
            return Ok(());
        }
        let sender = public_identity_text(&envelope.sender);
        let projection = self.projection();
        let sender_is_member = projection.contains(&sender);
        let sender_is_inviter = self.active()?.joining_inviter == Some(envelope.sender);

        match envelope.body {
            EnvelopeBody::JoinRequest {
                inviter,
                display_name,
            } => {
                if inviter != *self.identity.public_identity().as_bytes()
                    || !projection.contains(&self.identity.public_identity().to_string())
                {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission);
                }
                let head = projection
                    .canonical_head
                    .ok_or(WorkspaceSessionError::InvalidInviteAdmission)?;
                let operation = SignedMembershipOperation::add_member(
                    &self.identity,
                    self.active()?.summary.id.as_str(),
                    head,
                    self.active()?
                        .log
                        .next_author_counter(self.identity.public_identity().as_bytes()),
                    envelope.sender,
                    display_name,
                    now()?,
                )?;
                let operation_bytes = operation.encode()?;
                self.persist_operation(operation_bytes)?;
                self.send(
                    EnvelopeBody::MembershipSyncResponse(self.active()?.log.encoded_operations()),
                    envelope.workspace_id,
                )?;
            }
            EnvelopeBody::MembershipOperation(operation) => {
                if !sender_is_member && !sender_is_inviter {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission);
                }
                self.ensure_join_admission(&operation)?;
                self.persist_operation(operation)?;
            }
            EnvelopeBody::MembershipSyncRequest => {
                if !sender_is_member {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission);
                }
                let operations = self.active()?.log.encoded_operations();
                self.send(
                    EnvelopeBody::MembershipSyncResponse(operations),
                    envelope.workspace_id,
                )?;
            }
            EnvelopeBody::MembershipSyncResponse(operations) => {
                if !sender_is_member && !sender_is_inviter {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission);
                }
                for operation in operations {
                    self.ensure_join_admission(&operation)?;
                    self.persist_operation(operation)?;
                }
            }
            EnvelopeBody::Heartbeat { sent_at } => {
                if !sender_is_member {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission);
                }
                self.record_heartbeat(sender, sent_at)?;
            }
        }
        Ok(())
    }

    pub fn view(&mut self) -> Result<ActiveWorkspaceView, WorkspaceSessionError> {
        let projection = self.projection();
        let view = ActiveWorkspaceView {
            workspace: self.active()?.summary.clone(),
            local_public_identity: self.identity.public_identity().to_string(),
            peers: self.known_peers(&projection),
            members: projection.members,
        };
        self.transitions
            .push(WorkspaceTransition::WorkspaceChanged(view.clone()));
        Ok(view)
    }

    #[must_use]
    pub fn take_transitions(&mut self) -> Vec<WorkspaceTransition> {
        std::mem::take(&mut self.transitions)
    }

    pub(crate) fn transport_identity(&self) -> &InstallationIdentity {
        &self.identity
    }

    pub(crate) fn transport_settings(
        &self,
    ) -> Result<([u8; 32], Option<String>), WorkspaceSessionError> {
        let store = self.catalog.open_workspace(&self.active()?.summary.id)?;
        let settings = store.private_settings()?;
        Ok((*settings.token.as_bytes(), settings.relay_override))
    }

    pub(crate) fn transport_bootstrap(&self) -> Result<Option<&str>, WorkspaceSessionError> {
        Ok(self.active()?.bootstrap.as_deref())
    }

    pub fn delivery_mut(&mut self) -> &mut D {
        &mut self.delivery
    }

    #[must_use]
    pub fn into_delivery(self) -> D {
        self.delivery
    }

    fn record_heartbeat(
        &mut self,
        sender: String,
        sent_at: i64,
    ) -> Result<(), WorkspaceSessionError> {
        let peer = {
            let active = self.active_mut()?;
            let state = active.peers.entry(sender.clone()).or_insert(PeerState {
                last_heartbeat: sent_at,
                online: true,
                connection: PeerConnection::Unknown,
            });
            state.last_heartbeat = state.last_heartbeat.max(sent_at);
            state.online = true;
            KnownPeer {
                public_identity: sender,
                online: true,
                connection: state.connection.clone(),
            }
        };
        self.transitions
            .push(WorkspaceTransition::PeerPresenceChanged(peer));
        Ok(())
    }

    fn known_peers(&self, projection: &MembershipProjection) -> Vec<KnownPeer> {
        self.active
            .as_ref()
            .map(|active| {
                active
                    .peers
                    .iter()
                    .filter(|(public_identity, _)| projection.contains(public_identity))
                    .map(|(public_identity, state)| KnownPeer {
                        public_identity: public_identity.clone(),
                        online: state.online,
                        connection: state.connection.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn activate(&mut self, summary: WorkspaceSummary) -> Result<(), WorkspaceSessionError> {
        let store = self.catalog.open_workspace(&summary.id)?;
        let settings = store.private_settings()?;
        let mut log = MembershipLog::new();
        for operation in store.membership_operations()? {
            log.insert_bytes(&operation)?;
        }
        self.active = Some(ActiveWorkspace {
            summary,
            log,
            joining_inviter: settings.joining_inviter,
            bootstrap: settings.bootstrap,
            peers: BTreeMap::new(),
        });
        Ok(())
    }

    fn ensure_join_admission(&self, operation: &[u8]) -> Result<(), WorkspaceSessionError> {
        let Some(inviter) = self.active()?.joining_inviter else {
            return Ok(());
        };
        let signed = SignedMembershipOperation::decode(operation)?;
        let MembershipOperationBody::AddMember {
            public_identity,
            role,
            ..
        } = &signed.operation.body;
        if public_identity == self.identity.public_identity().as_bytes()
            && (signed.operation.author != inviter || role != "contributor")
        {
            return Err(WorkspaceSessionError::InvalidInviteAdmission);
        }
        Ok(())
    }

    fn persist_operation(&mut self, operation: Vec<u8>) -> Result<(), WorkspaceSessionError> {
        let signed = SignedMembershipOperation::decode(&operation)?;
        let operation_id = signed.operation_id()?;
        let before = self.projection();
        self.active_mut()?.log.insert(signed)?;
        let after = self.projection();
        let store = self.catalog.open_workspace(&self.active()?.summary.id)?;
        store.record_membership_operation(&operation_id, &operation)?;
        store.replace_members(&after.members)?;

        for member in &after.members {
            if !before.contains(&member.public_identity) {
                self.transitions
                    .push(WorkspaceTransition::MemberJoined(member.clone()));
            }
        }
        let local_identity = self.identity.public_identity().to_string();
        if self.active()?.summary.lifecycle == WorkspaceLifecycle::Joining
            && after.contains(&local_identity)
        {
            let id = self.active()?.summary.id.clone();
            self.catalog
                .set_workspace_lifecycle(&id, WorkspaceLifecycle::Ready)?;
            let store = self.catalog.open_workspace(&id)?;
            store.clear_pending_join_admission()?;
            self.active_mut()?.summary.lifecycle = WorkspaceLifecycle::Ready;
            self.active_mut()?.joining_inviter = None;
            self.active_mut()?.bootstrap = None;
        }
        Ok(())
    }

    fn send_join_request(&mut self, display_name: String) -> Result<(), WorkspaceSessionError> {
        let inviter = self
            .active()?
            .joining_inviter
            .ok_or(WorkspaceSessionError::InvalidInviteAdmission)?;
        let workspace_id = self.active()?.summary.id.as_str().to_owned();
        self.send(
            EnvelopeBody::JoinRequest {
                inviter,
                display_name,
            },
            workspace_id,
        )
    }

    fn send(
        &mut self,
        body: EnvelopeBody,
        workspace_id: String,
    ) -> Result<(), WorkspaceSessionError> {
        self.delivery
            .send(Envelope::sign(&self.identity, workspace_id, body)?.encode()?);
        Ok(())
    }

    fn active(&self) -> Result<&ActiveWorkspace, WorkspaceSessionError> {
        self.active
            .as_ref()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)
    }

    fn active_mut(&mut self) -> Result<&mut ActiveWorkspace, WorkspaceSessionError> {
        self.active
            .as_mut()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)
    }

    fn projection(&self) -> MembershipProjection {
        self.active.as_ref().map_or_else(
            || MembershipProjection {
                canonical_head: None,
                members: Vec::new(),
                statuses: Default::default(),
            },
            |active| active.log.projection(active.summary.id.as_str()),
        )
    }
}

fn public_identity_text(public_identity: &[u8; 32]) -> String {
    crate::identity::PublicIdentity::from_bytes(*public_identity).to_string()
}

fn now() -> Result<i64, WorkspaceSessionError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().try_into().unwrap_or(i64::MAX))
        .map_err(|_| WorkspaceSessionError::ClockUnavailable)
}
