//! Active-workspace state machine over a secret-free delivery port.

use std::{
    collections::BTreeMap,
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use crate::{
    identity::{InstallationIdentity, PublicIdentity},
    invite::{validate_relay_override, Invite, InviteError},
    membership_log::{
        MembershipError, MembershipLog, MembershipOperationBody, MembershipProjection,
        SignedMembershipOperation,
    },
    protocol::{Envelope, EnvelopeBody, ProtocolError},
    workspace_catalog::{WorkspaceCatalog, WorkspaceCatalogError},
    workspace_domain::{
        display_name as validate_display_name, KnownPeer, Member, PeerConnection,
        WorkspaceLifecycle, WorkspaceSummary, WorkspaceToken,
    },
    workspace_file_runtime::{WorkspaceFileRuntime, WorkspaceFileRuntimeError},
    workspace_files::{
        authority::WorkspaceFileAuthority, FileOperationBody, FileOperationError,
        SignedFileOperation,
    },
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
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
    pub local_public_identity: PublicIdentity,
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
    FileOperation(FileOperationError),
    Invite(InviteError),
    Protocol(ProtocolError),
    NoActiveWorkspace,
    InvalidInviteAdmission(&'static str),
    ClockUnavailable,
    InitializationRecovery(&'static str),
}

impl fmt::Display for WorkspaceSessionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "workspace catalog failed: {error}"),
            Self::Store(error) => write!(formatter, "workspace storage failed: {error}"),
            Self::Membership(error) => write!(formatter, "membership processing failed: {error}"),
            Self::FileOperation(error) => {
                write!(formatter, "workspace file processing failed: {error}")
            }
            Self::Invite(error) => write!(formatter, "invite processing failed: {error}"),
            Self::Protocol(error) => write!(formatter, "workspace protocol failed: {error}"),
            Self::NoActiveWorkspace => formatter.write_str("there is no active workspace"),
            Self::InvalidInviteAdmission(reason) => formatter.write_str(reason),
            Self::ClockUnavailable => formatter.write_str("system clock is unavailable"),
            Self::InitializationRecovery(reason) => formatter.write_str(reason),
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
impl From<FileOperationError> for WorkspaceSessionError {
    fn from(error: FileOperationError) -> Self {
        Self::FileOperation(error)
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
    joining_display_name: Option<String>,
    peers: BTreeMap<PublicIdentity, PeerState>,
    file_history_recovery_needed: bool,
}

pub(crate) struct FileRecoveryContext {
    pub workspace_id: crate::workspace_domain::WorkspaceId,
    pub membership: MembershipProjection,
    pub store: WorkspaceStore,
    pub bootstrap: Option<String>,
    pub member_public_identities: Vec<PublicIdentity>,
    pub local_public_identity: PublicIdentity,
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
    pub fn local_public_identity_value(&self) -> PublicIdentity {
        self.identity.public_identity()
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
        let initializing = summary.lifecycle == WorkspaceLifecycle::Initializing;
        self.activate(summary)?;
        if initializing {
            self.complete_initialization()?;
        }
        self.view().map(Some)
    }

    pub fn create_workspace(
        &mut self,
        display_name: impl Into<String>,
        relay_override: Option<String>,
    ) -> Result<ActiveWorkspaceView, WorkspaceSessionError> {
        self.create_workspace_with_creator(
            display_name,
            self.identity.public_identity().to_string(),
            relay_override,
        )
    }

    pub fn create_workspace_with_creator(
        &mut self,
        display_name: impl Into<String>,
        creator_display_name: impl Into<String>,
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
            WorkspaceLifecycle::Initializing,
        )?;
        let creator_display_name =
            validate_display_name(creator_display_name).map_err(WorkspaceCatalogError::Domain)?;
        let store = self
            .catalog
            .open_workspace_for_initialization(&summary.id)?;
        store.set_creation_creator_display_name(&creator_display_name)?;
        self.activate(summary)?;
        self.complete_initialization()?;
        self.view()
    }

    pub fn create_invite(
        &self,
        bootstrap: impl Into<String>,
    ) -> Result<String, WorkspaceSessionError> {
        let active = self.active()?;
        if active.summary.lifecycle != WorkspaceLifecycle::Ready {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization must finish before creating an invite",
            ));
        }
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
        let display_name =
            validate_display_name(display_name).map_err(WorkspaceCatalogError::Domain)?;
        let store = self.catalog.open_workspace(&summary.id)?;
        store.set_pending_join_admission(invite.inviter(), invite.bootstrap(), &display_name)?;
        self.activate(summary)?;
        self.send_join_request(display_name)?;
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
        let display_name =
            validate_display_name(display_name).map_err(WorkspaceCatalogError::Domain)?;
        self.set_pending_join_display_name(&display_name)?;
        self.send_join_request(display_name)?;
        Ok(true)
    }

    pub fn request_membership_sync(&mut self) -> Result<(), WorkspaceSessionError> {
        if self.active()?.summary.lifecycle == WorkspaceLifecycle::Initializing {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization must finish before transport starts",
            ));
        }
        let workspace_id = self.active()?.summary.id.as_str().to_owned();
        self.send(EnvelopeBody::MembershipSyncRequest, workspace_id)
    }

    pub fn announce_file_history(
        &mut self,
        operation_ids: Vec<String>,
    ) -> Result<(), WorkspaceSessionError> {
        if !self.is_ready()? {
            return Ok(());
        }
        let workspace_id = self.active()?.summary.id.as_str().to_owned();
        self.send(
            EnvelopeBody::FileHistoryNotice { operation_ids },
            workspace_id,
        )
    }

    pub fn take_file_history_recovery_needed(&mut self) -> Result<bool, WorkspaceSessionError> {
        let active = self.active_mut()?;
        Ok(std::mem::take(&mut active.file_history_recovery_needed))
    }

    pub(crate) fn file_recovery_context(
        &self,
    ) -> Result<FileRecoveryContext, WorkspaceSessionError> {
        let active = self.active()?;
        let membership = self.projection();
        Ok(FileRecoveryContext {
            workspace_id: active.summary.id.clone(),
            store: self.catalog.open_workspace(&active.summary.id)?,
            bootstrap: active.bootstrap.clone(),
            member_public_identities: membership
                .members
                .iter()
                .map(|member| member.public_identity)
                .collect(),
            local_public_identity: self.identity.public_identity(),
            membership,
        })
    }

    pub(crate) fn mark_file_history_recovery_needed(
        &mut self,
    ) -> Result<(), WorkspaceSessionError> {
        self.active_mut()?.file_history_recovery_needed = true;
        Ok(())
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
        let public_identity = PublicIdentity::from_bytes(public_identity);
        if !self.projection().contains(&public_identity) {
            return Ok(());
        }
        let peer = {
            let active = self.active_mut()?;
            let state = active.peers.entry(public_identity).or_insert(PeerState {
                last_heartbeat: 0,
                online: false,
                connection: PeerConnection::Unknown,
            });
            if state.connection == connection {
                return Ok(());
            }
            state.connection = connection;
            KnownPeer {
                public_identity,
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
                        public_identity: *public_identity,
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
        if self.active()?.summary.lifecycle == WorkspaceLifecycle::Initializing {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization must finish before receiving transport data",
            ));
        }
        let envelope = Envelope::decode(bytes)?;
        envelope.verify()?;
        let envelope_workspace_id = envelope.workspace_id_value()?;
        if envelope_workspace_id != self.active()?.summary.id {
            return Err(ProtocolError::InvalidWorkspace.into());
        }
        // Gossip may deliver a publisher's signed envelope back to that same
        // publisher. Local state was already changed before the broadcast.
        if envelope.sender == *self.identity.public_identity().as_bytes() {
            return Ok(());
        }
        let sender_identity = PublicIdentity::from_bytes(envelope.sender);
        let projection = self.projection();
        let sender_is_member = projection.contains(&sender_identity);
        let sender_is_inviter = self.active()?.joining_inviter == Some(envelope.sender);

        match envelope.body {
            EnvelopeBody::JoinRequest {
                inviter,
                display_name,
            } => {
                if inviter != *self.identity.public_identity().as_bytes()
                    || !projection.contains(&self.identity.public_identity())
                {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission(
                        "join request is not addressed to this installation",
                    ));
                }
                let head = projection.canonical_head.ok_or(
                    WorkspaceSessionError::InvalidInviteAdmission(
                        "inviter has no canonical membership authority",
                    ),
                )?;
                let operation = SignedMembershipOperation::add_member(
                    &self.identity,
                    self.active()?.summary.id.as_str(),
                    head.as_str(),
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
                    EnvelopeBody::MembershipSyncResponse(self.active()?.log.encoded_operations()?),
                    envelope.workspace_id,
                )?;
            }
            EnvelopeBody::MembershipOperation(operation) => {
                if !sender_is_member && !sender_is_inviter {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission(
                        "membership operation is not from the inviter or a member",
                    ));
                }
                self.ensure_join_admission(&operation)?;
                self.persist_operation(operation)?;
            }
            EnvelopeBody::MembershipSyncRequest => {
                // A ready inviter can ask for sync as soon as the Gossip
                // neighbor appears. A pending joiner has not necessarily
                // received the inviter's genesis record yet, so it cannot
                // authorize or answer that request until admission completes.
                if sender_is_member {
                    let operations = self.active()?.log.encoded_operations()?;
                    self.send(
                        EnvelopeBody::MembershipSyncResponse(operations),
                        envelope.workspace_id,
                    )?;
                }
            }
            EnvelopeBody::MembershipSyncResponse(operations) => {
                if !sender_is_member && !sender_is_inviter {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission(
                        "membership sync response is not from the inviter or a member",
                    ));
                }
                for operation in operations {
                    self.ensure_join_admission(&operation)?;
                    self.persist_operation(operation)?;
                }
            }
            EnvelopeBody::FileHistoryNotice { .. } => {
                if !sender_is_member {
                    return Err(WorkspaceSessionError::InvalidInviteAdmission(
                        "file-history notice is not from a member",
                    ));
                }
                self.mark_file_history_recovery_needed()?;
            }
            EnvelopeBody::Heartbeat { sent_at } => {
                // An inviter can be connected before its genesis/member record
                // reaches a pending joiner. Its early heartbeat grants no
                // authority and is safely ignored until membership is known.
                if sender_is_member {
                    self.record_heartbeat(sender_identity, sent_at)?;
                }
            }
        }
        Ok(())
    }

    pub fn open_file_runtime(&self) -> Result<WorkspaceFileRuntime, WorkspaceFileRuntimeError> {
        let active = self
            .active
            .as_ref()
            .ok_or(WorkspaceFileRuntimeError::NotFound)?;
        if active.summary.lifecycle != WorkspaceLifecycle::Ready {
            return Err(WorkspaceFileRuntimeError::NotFound);
        }
        let store = self.catalog.open_workspace(&active.summary.id)?;
        WorkspaceFileRuntime::open(
            active.summary.id.as_str(),
            self.identity.clone(),
            self.projection(),
            store,
        )
    }

    pub fn view(&mut self) -> Result<ActiveWorkspaceView, WorkspaceSessionError> {
        let projection = self.projection();
        let view = ActiveWorkspaceView {
            workspace: self.active()?.summary.clone(),
            local_public_identity: self.identity.public_identity(),
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

    pub(crate) fn pending_transition_count(&self) -> usize {
        self.transitions.len()
    }

    pub(crate) fn transport_identity(&self) -> &InstallationIdentity {
        &self.identity
    }

    pub(crate) fn transport_settings(
        &self,
    ) -> Result<([u8; 32], Option<String>), WorkspaceSessionError> {
        if self.active()?.summary.lifecycle == WorkspaceLifecycle::Initializing {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization must finish before transport starts",
            ));
        }
        let store = self.catalog.open_workspace(&self.active()?.summary.id)?;
        let settings = store.private_settings()?;
        Ok((*settings.token.as_bytes(), settings.relay_override))
    }

    pub(crate) fn transport_bootstrap(&self) -> Result<Option<&str>, WorkspaceSessionError> {
        Ok(self.active()?.bootstrap.as_deref())
    }

    pub(crate) fn resend_pending_join(&mut self) -> Result<bool, WorkspaceSessionError> {
        if self.is_ready()? {
            return Ok(false);
        }
        let display_name = self.active()?.joining_display_name.clone().ok_or(
            WorkspaceSessionError::InvalidInviteAdmission("pending join has no display name"),
        )?;
        self.send_join_request(display_name)?;
        Ok(true)
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
        sender: PublicIdentity,
        sent_at: i64,
    ) -> Result<(), WorkspaceSessionError> {
        let peer = {
            let active = self.active_mut()?;
            let state = active.peers.entry(sender).or_insert(PeerState {
                last_heartbeat: sent_at,
                online: false,
                connection: PeerConnection::Unknown,
            });
            state.last_heartbeat = state.last_heartbeat.max(sent_at);
            if state.online {
                return Ok(());
            }
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
                        public_identity: *public_identity,
                        online: state.online,
                        connection: state.connection.clone(),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn activate(&mut self, summary: WorkspaceSummary) -> Result<(), WorkspaceSessionError> {
        let recover_file_history = summary.lifecycle == WorkspaceLifecycle::Ready;
        let store = self
            .catalog
            .open_workspace_for_initialization(&summary.id)?;
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
            joining_display_name: settings.joining_display_name,
            peers: BTreeMap::new(),
            file_history_recovery_needed: recover_file_history,
        });
        Ok(())
    }

    fn complete_initialization(&mut self) -> Result<(), WorkspaceSessionError> {
        if self.active()?.summary.lifecycle != WorkspaceLifecycle::Initializing {
            return Ok(());
        }
        let id = self.active()?.summary.id.clone();
        let workspace_id = id.as_str().to_owned();
        let store = self.catalog.open_workspace_for_initialization(&id)?;
        let settings = store.private_settings()?;
        if settings.creation_stage == "ready" {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization has an inconsistent durable stage",
            ));
        }
        let creator_display_name = settings.creation_creator_display_name.ok_or(
            WorkspaceSessionError::InitializationRecovery(
                "workspace initialization has no durable creator display name",
            ),
        )?;

        if store.membership_operations()?.is_empty() {
            let genesis = SignedMembershipOperation::genesis(
                &self.identity,
                workspace_id.clone(),
                creator_display_name,
                now()?,
            )?;
            self.persist_operation(genesis.encode()?)?;
            store.set_creation_stage("membership-initialized")?;
        }
        let membership = self.projection();
        if !membership.contains(&self.identity.public_identity()) {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization membership genesis is not owned by this installation",
            ));
        }

        let mut file_operations = store.file_operations()?;
        let initial_count = file_operations
            .iter()
            .filter(|operation| {
                matches!(
                    &operation.operation.body,
                    FileOperationBody::CreateDirectory {
                        parent_node_id: None,
                        name,
                    } if name == "plans"
                )
            })
            .count();
        if initial_count == 0 {
            let initial_directory = SignedFileOperation::create_directory(
                &self.identity,
                workspace_id.clone(),
                None,
                "plans",
                Vec::new(),
            )?;
            store.record_file_operation(&initial_directory)?;
            file_operations.push(initial_directory);
            store.set_creation_stage("files-initialized")?;
        } else if initial_count != 1 {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization contains duplicate plans operations",
            ));
        }
        store.set_creation_stage("files-initialized")?;

        let mut authority = WorkspaceFileAuthority::new(&workspace_id);
        authority
            .replay(&file_operations, &membership)
            .map_err(|_| {
                WorkspaceSessionError::InitializationRecovery(
                    "workspace initialization file history cannot be projected",
                )
            })?;
        if !authority.projection().root.contains_key("plans") {
            return Err(WorkspaceSessionError::InitializationRecovery(
                "workspace initialization did not project plans",
            ));
        }

        store.set_lifecycle(&WorkspaceLifecycle::Ready)?;
        self.catalog.publish_workspace(&id)?;
        self.active_mut()?.summary.lifecycle = WorkspaceLifecycle::Ready;
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
            return Err(WorkspaceSessionError::InvalidInviteAdmission(
                "membership admission is not signed by the named inviter",
            ));
        }
        Ok(())
    }

    fn persist_operation(&mut self, operation: Vec<u8>) -> Result<(), WorkspaceSessionError> {
        let signed = SignedMembershipOperation::decode(&operation)?;
        let operation_id = signed.operation_id_value()?;
        let before = self.projection();
        self.active_mut()?.log.insert(signed)?;
        let after = self.projection();
        let store = self
            .catalog
            .open_workspace_for_initialization(&self.active()?.summary.id)?;
        store.record_membership_operation(&operation_id, &operation)?;
        store.replace_members(&after.members)?;

        for member in &after.members {
            if !before.contains(&member.public_identity) {
                self.transitions
                    .push(WorkspaceTransition::MemberJoined(member.clone()));
            }
        }
        if self.active()?.summary.lifecycle == WorkspaceLifecycle::Joining
            && after.contains(&self.identity.public_identity())
        {
            let id = self.active()?.summary.id.clone();
            self.catalog
                .set_workspace_lifecycle(&id, WorkspaceLifecycle::Ready)?;
            let store = self.catalog.open_workspace(&id)?;
            store.clear_pending_join_admission()?;
            self.active_mut()?.summary.lifecycle = WorkspaceLifecycle::Ready;
            self.active_mut()?.joining_inviter = None;
            self.active_mut()?.joining_display_name = None;
            self.active_mut()?.file_history_recovery_needed = true;
        }
        Ok(())
    }

    fn set_pending_join_display_name(
        &mut self,
        display_name: &str,
    ) -> Result<(), WorkspaceSessionError> {
        let id = self.active()?.summary.id.clone();
        let inviter =
            self.active()?
                .joining_inviter
                .ok_or(WorkspaceSessionError::InvalidInviteAdmission(
                    "pending join has no named inviter",
                ))?;
        let bootstrap = self.active()?.bootstrap.clone().ok_or(
            WorkspaceSessionError::InvalidInviteAdmission("pending join has no bootstrap address"),
        )?;
        let store = self.catalog.open_workspace(&id)?;
        store.set_pending_join_admission(inviter, &bootstrap, display_name)?;
        self.active_mut()?.joining_display_name = Some(display_name.to_owned());
        Ok(())
    }

    fn send_join_request(&mut self, display_name: String) -> Result<(), WorkspaceSessionError> {
        let inviter =
            self.active()?
                .joining_inviter
                .ok_or(WorkspaceSessionError::InvalidInviteAdmission(
                    "pending join has no named inviter",
                ))?;
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

fn now() -> Result<i64, WorkspaceSessionError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().try_into().unwrap_or(i64::MAX))
        .map_err(|_| WorkspaceSessionError::ClockUnavailable)
}

#[cfg(test)]
mod tests {
    use crate::{
        identity::{InMemoryKeyCustody, InstallationIdentity},
        workspace_catalog::WorkspaceCatalog,
    };

    use super::{FakeDeliveryPort, WorkspaceSession};

    #[test]
    fn retains_an_admitted_inviter_as_a_restart_bootstrap_peer() {
        let inviter_directory = tempfile::tempdir().expect("inviter directory creates");
        let joiner_directory = tempfile::tempdir().expect("joiner directory creates");
        let inviter_custody = InMemoryKeyCustody::default();
        let joiner_custody = InMemoryKeyCustody::default();
        let mut inviter = WorkspaceSession::new(
            InstallationIdentity::load_or_create(&inviter_custody)
                .expect("inviter identity creates"),
            WorkspaceCatalog::open(inviter_directory.path()).expect("inviter catalog opens"),
            FakeDeliveryPort::default(),
        );
        inviter
            .create_workspace("Team Resonance", None)
            .expect("workspace creates");
        let invite = inviter.create_invite("bootstrap").expect("invite creates");
        let mut joiner = WorkspaceSession::new(
            InstallationIdentity::load_or_create(&joiner_custody).expect("joiner identity creates"),
            WorkspaceCatalog::open(joiner_directory.path()).expect("joiner catalog opens"),
            FakeDeliveryPort::default(),
        );
        joiner.join_workspace(&invite, "Lin").expect("join starts");
        let join_request = joiner
            .delivery_mut()
            .take_outbound()
            .pop()
            .expect("join request queues");
        inviter.receive(&join_request).expect("inviter admits");
        let admission = inviter
            .delivery_mut()
            .take_outbound()
            .pop()
            .expect("admission queues");
        joiner.receive(&admission).expect("joiner is admitted");

        assert_eq!(
            joiner
                .transport_bootstrap()
                .expect("bootstrap is available"),
            Some("bootstrap")
        );
        drop(joiner);

        let mut restarted_joiner = WorkspaceSession::new(
            InstallationIdentity::load_or_create(&joiner_custody).expect("joiner identity reloads"),
            WorkspaceCatalog::open(joiner_directory.path()).expect("joiner catalog reopens"),
            FakeDeliveryPort::default(),
        );
        restarted_joiner
            .activate_active_workspace()
            .expect("workspace activates");

        assert_eq!(
            restarted_joiner
                .transport_bootstrap()
                .expect("persisted bootstrap is available"),
            Some("bootstrap")
        );
    }
}
