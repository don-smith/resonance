//! Active-workspace state machine over a secret-free delivery port.

use std::{
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
    workspace_domain::{Member, WorkspaceLifecycle, WorkspaceSummary, WorkspaceToken},
    workspace_store::WorkspaceStoreError,
};

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkspaceTransition {
    WorkspaceChanged(ActiveWorkspaceView),
    MemberJoined(Member),
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
        self.activate(summary, None)?;
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
        self.activate(summary, Some(invite.inviter()))?;
        self.send_join_request(display_name.into())?;
        debug_assert_eq!(self.active()?.summary.id.as_str(), workspace_id);
        self.view()
    }

    pub fn retry_join(
        &mut self,
        display_name: impl Into<String>,
    ) -> Result<(), WorkspaceSessionError> {
        if self.active()?.summary.lifecycle != WorkspaceLifecycle::Joining {
            return Err(WorkspaceSessionError::InvalidInviteAdmission);
        }
        self.send_join_request(display_name.into())
    }

    pub fn request_membership_sync(&mut self) -> Result<(), WorkspaceSessionError> {
        let workspace_id = self.active()?.summary.id.as_str().to_owned();
        self.send(EnvelopeBody::MembershipSyncRequest, workspace_id)
    }

    pub fn receive(&mut self, bytes: &[u8]) -> Result<(), WorkspaceSessionError> {
        let envelope = Envelope::decode(bytes)?;
        envelope.verify()?;
        if envelope.workspace_id != self.active()?.summary.id.as_str() {
            return Err(ProtocolError::InvalidWorkspace.into());
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
        }
        Ok(())
    }

    pub fn view(&mut self) -> Result<ActiveWorkspaceView, WorkspaceSessionError> {
        let projection = self.projection();
        let view = ActiveWorkspaceView {
            workspace: self.active()?.summary.clone(),
            local_public_identity: self.identity.public_identity().to_string(),
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

    pub fn delivery_mut(&mut self) -> &mut D {
        &mut self.delivery
    }

    #[must_use]
    pub fn into_delivery(self) -> D {
        self.delivery
    }

    fn activate(
        &mut self,
        summary: WorkspaceSummary,
        joining_inviter: Option<[u8; 32]>,
    ) -> Result<(), WorkspaceSessionError> {
        let store = self.catalog.open_workspace(&summary.id)?;
        let mut log = MembershipLog::new();
        for operation in store.membership_operations()? {
            log.insert_bytes(&operation)?;
        }
        self.active = Some(ActiveWorkspace {
            summary,
            log,
            joining_inviter,
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
            self.active_mut()?.summary.lifecycle = WorkspaceLifecycle::Ready;
            self.active_mut()?.joining_inviter = None;
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
