//! Runtime-owned application service for the active workspace.
//!
//! This module owns session, transport, file-runtime, and health state. The
//! desktop crate translates its views and errors into Tauri wire contracts.

use std::{
    net::SocketAddr,
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use tokio::{
    sync::{oneshot, Mutex},
    task::JoinHandle,
    time,
};

use crate::{
    conversations::{
        authority::ConversationAuthority,
        lookup::ConversationLookup,
        mesh::ConversationMeshEvent,
        runtime::ConversationRuntime,
        wire::{ConversationRecordV1, ExactRecordV1},
    },
    identity::{IdentityError, InstallationIdentity, PublicIdentity},
    iroh_transport::{IrohSessionAdapterError, IrohTransport, IrohTransportError},
    membership_log::{PreparedMembershipTransition, SignedSelfRemovalRequestV1},
    workspace_catalog::WorkspaceCatalog,
    workspace_domain::{KnownPeer, Member, WorkspaceSummary},
    workspace_file_runtime::WorkspaceFileRuntime,
    workspace_session::{
        ConversationControl, FakeDeliveryPort, WorkspaceSession, WorkspaceSessionError,
        WorkspaceTransition,
    },
};

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
pub const TRANSPORT_POLL_INTERVAL: Duration = Duration::from_millis(500);
const RETRY_BACKOFF: Duration = Duration::from_millis(250);
const MAX_CONVERSATION_MESH_EVENTS_PER_POLL: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceApplicationUpdate {
    View,
    Files,
    Conversations,
}

pub struct WorkspaceLifecycleHandle {
    cancel: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl WorkspaceLifecycleHandle {
    pub async fn stop(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceHealth {
    pub identity: Option<String>,
    pub storage_unavailable: bool,
    pub network: Option<String>,
}

impl WorkspaceHealth {
    fn identity_error(message: String) -> Self {
        Self {
            identity: Some(message),
            ..Self::default()
        }
    }

    fn storage_error() -> Self {
        Self {
            storage_unavailable: true,
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceApplicationView {
    pub revision: u64,
    pub workspace: Option<WorkspaceSummary>,
    pub local_public_identity: Option<PublicIdentity>,
    pub members: Vec<Member>,
    pub peers: Vec<KnownPeer>,
    pub health: WorkspaceHealth,
}

pub struct WorkspaceApplication {
    session: Option<WorkspaceSession<FakeDeliveryPort>>,
    transport: Option<IrohTransport>,
    conversation: Option<ConversationRuntime>,
    conversation_lookup: Option<ConversationLookup>,
    conversation_address_generation: u64,
    conversation_address_expires_at: i64,
    pub(crate) files: Option<WorkspaceFileRuntime>,
    health: WorkspaceHealth,
    local_public_identity: Option<PublicIdentity>,
    last_view: Option<WorkspaceApplicationView>,
    view_revision: AtomicU64,
}

impl WorkspaceApplication {
    pub fn initialize(
        identity: Result<InstallationIdentity, IdentityError>,
        application_data: &Path,
    ) -> Self {
        let (session, files, conversation, local_public_identity, health) = match identity {
            Ok(identity) => match WorkspaceCatalog::open(application_data) {
                Ok(catalog) => {
                    let mut session =
                        WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
                    let local_public_identity = session.local_public_identity_value();
                    match session.activate_active_workspace() {
                        Ok(_) => {
                            let files = session.open_file_runtime().ok();
                            let conversation = session
                                .conversation_runtime_context()
                                .ok()
                                .and_then(|(identity, workspace, store, membership)| {
                                    ConversationRuntime::open(
                                        identity, workspace, store, membership,
                                    )
                                    .ok()
                                });
                            (
                                Some(session),
                                files,
                                conversation,
                                Some(local_public_identity),
                                WorkspaceHealth::default(),
                            )
                        }
                        Err(_) => (None, None, None, None, WorkspaceHealth::storage_error()),
                    }
                }
                Err(_) => (None, None, None, None, WorkspaceHealth::storage_error()),
            },
            Err(error) => (
                None,
                None,
                None,
                None,
                WorkspaceHealth::identity_error(error.to_string()),
            ),
        };
        let mut health = health;
        if session
            .as_ref()
            .is_some_and(WorkspaceSession::has_active_workspace)
            && conversation.is_none()
        {
            health.network = Some("Conversation durable state could not be opened.".to_owned());
        }
        Self {
            session,
            transport: None,
            conversation,
            conversation_lookup: None,
            conversation_address_generation: 0,
            conversation_address_expires_at: 0,
            files,
            health,
            local_public_identity,
            last_view: None,
            view_revision: AtomicU64::new(0),
        }
    }

    pub fn view(&mut self) -> WorkspaceApplicationView {
        let (workspace, local_public_identity, members, peers) = match self.session.as_mut() {
            None => (None, None, Vec::new(), Vec::new()),
            Some(session) if !session.has_active_workspace() => {
                (None, self.local_public_identity, Vec::new(), Vec::new())
            }
            Some(session) => match session.view() {
                Ok(view) => (
                    Some(view.workspace),
                    Some(view.local_public_identity),
                    view.members,
                    view.peers,
                ),
                Err(_) => (None, self.local_public_identity, Vec::new(), Vec::new()),
            },
        };
        let mut view = WorkspaceApplicationView {
            revision: 0,
            workspace,
            local_public_identity,
            members,
            peers,
            health: self.health.clone(),
        };
        let changed = self.last_view.as_ref().is_none_or(|previous| {
            let mut comparable = previous.clone();
            comparable.revision = 0;
            comparable != view
        });
        if changed {
            self.view_revision.fetch_add(1, Ordering::Relaxed);
        }
        view.revision = self.view_revision.load(Ordering::Relaxed);
        self.last_view = Some(view.clone());
        view
    }

    pub fn conversation(&self) -> Option<&ConversationRuntime> {
        self.conversation.as_ref()
    }

    pub fn conversation_mut(&mut self) -> Option<&mut ConversationRuntime> {
        self.conversation.as_mut()
    }

    pub fn conversation_synchronization_state(
        &mut self,
    ) -> Result<
        crate::conversations::runtime::ConversationSyncState,
        crate::conversations::runtime::ConversationRuntimeError,
    > {
        let has_direct_candidate = self
            .conversation_lookup
            .as_mut()
            .is_some_and(|lookup| lookup.another_member_has_candidate(unix_seconds()));
        let has_usable_peer = self
            .conversation_lookup
            .as_ref()
            .is_some_and(|lookup| lookup.has_recent_usable_peer(unix_seconds()));
        self.conversation
            .as_ref()
            .ok_or(crate::conversations::runtime::ConversationRuntimeError::InvalidWorkspace)?
            .network_synchronization_state(has_direct_candidate, has_usable_peer)
    }

    pub fn take_transitions(&mut self) -> Vec<WorkspaceTransition> {
        self.session
            .as_mut()
            .map(WorkspaceSession::take_transitions)
            .unwrap_or_default()
    }

    pub fn create_workspace(
        &mut self,
        display_name: String,
        creator_display_name: String,
        relay_override: Option<String>,
    ) -> Result<(), WorkspaceSessionError> {
        let session = self
            .session
            .as_mut()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?;
        session.create_workspace_with_creator(
            display_name,
            creator_display_name,
            relay_override,
        )?;
        self.refresh_file_runtime()?;
        self.refresh_conversation_runtime()?;
        self.health = WorkspaceHealth::default();
        Ok(())
    }

    pub fn request_departure(
        &self,
        requested_at: i64,
    ) -> Result<SignedSelfRemovalRequestV1, WorkspaceSessionError> {
        self.session
            .as_ref()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?
            .request_departure(requested_at)
    }

    pub fn process_departure_request(
        &mut self,
        request: SignedSelfRemovalRequestV1,
        removed_at: i64,
    ) -> Result<(), WorkspaceSessionError> {
        let prepared = self
            .session
            .as_ref()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?
            .prepare_requested_departure(request, removed_at)?;
        self.commit_membership_transition(&prepared)
    }

    pub fn expel_member(
        &mut self,
        target: PublicIdentity,
        removed_at: i64,
    ) -> Result<(), WorkspaceSessionError> {
        let prepared = self
            .session
            .as_ref()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?
            .prepare_creator_expulsion(target, removed_at)?;
        self.commit_membership_transition(&prepared)
    }

    fn commit_membership_transition(
        &mut self,
        prepared: &PreparedMembershipTransition,
    ) -> Result<(), WorkspaceSessionError> {
        let session = self
            .session
            .as_mut()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?;
        let (identity, workspace_id, store) = session.conversation_authority_context()?;
        ConversationAuthority::open(identity, &workspace_id, store)?.commit_transition(prepared)?;
        session.finalize_prepared_transition(prepared)?;
        let (_, _, _, membership) = session.conversation_runtime_context()?;
        if let Some(conversation) = self.conversation.as_mut() {
            conversation.replace_membership(membership).map_err(|_| {
                WorkspaceSessionError::InitializationRecovery(
                    "conversation runtime could not apply membership",
                )
            })?;
        }
        Ok(())
    }

    pub fn create_invite(&self, bootstrap: String) -> Result<String, WorkspaceSessionError> {
        self.session
            .as_ref()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?
            .create_invite(bootstrap)
    }

    pub fn join_workspace(
        &mut self,
        invite: &str,
        display_name: String,
    ) -> Result<(), WorkspaceSessionError> {
        self.session
            .as_mut()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?
            .join_workspace(invite, display_name)?;
        self.refresh_file_runtime()?;
        self.refresh_conversation_runtime()?;
        Ok(())
    }

    pub fn retry_join(&mut self, display_name: String) -> Result<bool, WorkspaceSessionError> {
        self.session
            .as_mut()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?
            .retry_join(display_name)
    }

    fn refresh_conversation_runtime(&mut self) -> Result<(), WorkspaceSessionError> {
        let session = self
            .session
            .as_ref()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?;
        let (identity, workspace, store, membership) = session.conversation_runtime_context()?;
        self.conversation = Some(
            ConversationRuntime::open(identity, workspace, store, membership).map_err(|_| {
                WorkspaceSessionError::InitializationRecovery(
                    "conversation runtime could not open durable state",
                )
            })?,
        );
        Ok(())
    }

    pub fn refresh_file_runtime(&mut self) -> Result<(), WorkspaceSessionError> {
        self.files = self
            .session
            .as_ref()
            .and_then(|session| session.open_file_runtime().ok());
        Ok(())
    }

    pub async fn restart_transport(&mut self) {
        if let Some(mut lookup) = self.conversation_lookup.take() {
            let _ = lookup.stop();
        }
        if let Some(mut active) = self.transport.take() {
            let _ = active.shutdown().await;
        }
        let Some(session) = self.session.as_mut() else {
            return;
        };
        match IrohTransport::start_for_session(session).await {
            Ok(active) => {
                if let Some(files) = self.files.as_ref() {
                    if let Ok(service) = files.recovery_service() {
                        active.configure_file_recovery(service).await;
                    }
                }
                let conversation_start = async {
                    let (identity, workspace, store, membership) =
                        session.conversation_runtime_context()?;
                    if !conversation_membership_is_ready(&identity, &workspace, &membership) {
                        return Ok::<_, WorkspaceSessionError>(None);
                    }
                    let candidates = active.direct_socket_candidates().await;
                    if self.conversation.is_none() {
                        self.conversation = Some(
                            ConversationRuntime::open(
                                identity.clone(),
                                &workspace,
                                store.clone(),
                                membership.clone(),
                            )
                            .map_err(|_| {
                                WorkspaceSessionError::InitializationRecovery(
                                    "conversation runtime could not open durable state",
                                )
                            })?,
                        );
                    }
                    let version = store.lookup_peer_set_version()?;
                    let generation = store.next_conversation_address_generation()?;
                    let mut lookup = ConversationLookup::start_with_store(
                        identity,
                        &workspace,
                        membership.clone(),
                        "0.0.0.0:0".parse().expect("fixed listener address parses"),
                        store,
                        unix_seconds(),
                    )
                    .map_err(|_| {
                        WorkspaceSessionError::InitializationRecovery(
                            "conversation lookup could not start",
                        )
                    })?;
                    lookup
                        .replace_membership(membership, version, unix_seconds())
                        .map_err(|_| {
                            WorkspaceSessionError::InitializationRecovery(
                                "conversation peer set could not start",
                            )
                        })?;
                    let port = lookup.mesh().listen_addr().port();
                    let mut advertised = candidates
                        .into_iter()
                        .map(|mut address| {
                            address.set_port(port);
                            address
                        })
                        .collect::<Vec<_>>();
                    if advertised.is_empty() {
                        advertised.push(SocketAddr::from(([127, 0, 0, 1], port)));
                    }
                    self.conversation_address_generation = generation;
                    self.conversation_address_expires_at = unix_seconds().saturating_add(60);
                    let notice = lookup
                        .local_address_notice_with_addresses(
                            self.conversation_address_generation,
                            self.conversation_address_expires_at,
                            advertised,
                        )
                        .map_err(|_| {
                            WorkspaceSessionError::InitializationRecovery(
                                "conversation address notice could not be signed",
                            )
                        })?;
                    session.announce_address_notice(notice.bytes().to_vec())?;
                    session.announce_recipient_key()?;
                    Ok::<_, WorkspaceSessionError>(Some(lookup))
                }
                .await;
                match conversation_start {
                    Ok(Some(lookup)) => self.conversation_lookup = Some(lookup),
                    Ok(None) => self.health.network = None,
                    Err(error) => self.health.network = Some(error.to_string()),
                }
                if let Err(error) = active.flush_session(session).await {
                    self.health.network = Some(network_delivery_message(error));
                } else if self.conversation_lookup.is_some() {
                    self.health.network = None;
                }
                self.transport = Some(active);
            }
            Err(error) => self.health.network = Some(network_start_message(error)),
        }
    }

    pub async fn poll_transport(&mut self) -> bool {
        let mut changed = {
            let Some(transport) = self.transport.as_mut() else {
                return false;
            };
            let Some(session) = self.session.as_mut() else {
                return false;
            };
            let result = time::timeout(
                TRANSPORT_POLL_INTERVAL,
                transport.apply_next_session_event(session),
            )
            .await;
            match result {
                Ok(Ok(view_changed)) => {
                    let mut changed = view_changed;
                    if let Err(error) = transport.flush_session(session).await {
                        self.health.network = Some(network_delivery_message(error));
                        changed = true;
                    }
                    match transport.recover_file_history(session).await {
                        Ok(recovered) => changed |= recovered,
                        Err(error) => {
                            self.health.network = Some(network_delivery_message(error));
                            changed = true;
                        }
                    }
                    let controls = session.take_conversation_controls();
                    if let Ok((_, _, store, membership)) = session.conversation_runtime_context() {
                        if let Some(conversation) = self.conversation.as_mut() {
                            if conversation.replace_membership(membership.clone()).is_err() {
                                self.health.network = Some(
                                    "Conversation authority could not apply current membership."
                                        .to_owned(),
                                );
                            }
                        }
                        if let Some(lookup) = self.conversation_lookup.as_mut() {
                            let peer_update_failed =
                                store.lookup_peer_set_version().ok().is_none_or(|version| {
                                    lookup
                                        .replace_membership(membership, version, unix_seconds())
                                        .is_err()
                                });
                            if peer_update_failed {
                                self.health.network =
                                    Some("Conversation peer set could not be updated.".to_owned());
                            }
                            for control in controls {
                                let ConversationControl::AddressNotice {
                                    authenticated_sender,
                                    exact_bytes,
                                } = control;
                                if lookup
                                    .accept_address_notice(
                                        authenticated_sender,
                                        &exact_bytes,
                                        unix_seconds(),
                                    )
                                    .is_err()
                                {
                                    self.health.network = Some(
                                        "Conversation address control was rejected.".to_owned(),
                                    );
                                }
                            }
                        }
                    }
                    if self.conversation_lookup.is_none()
                        && session.conversation_runtime_context().is_ok_and(
                            |(identity, workspace, _, membership)| {
                                conversation_membership_is_ready(&identity, &workspace, &membership)
                            },
                        )
                    {
                        self.restart_transport().await;
                        return true;
                    }
                    if view_changed {
                        let conversation_port = self
                            .conversation_lookup
                            .as_ref()
                            .map(|lookup| lookup.mesh().listen_addr().port());
                        if let Some(port) = conversation_port {
                            let mut advertised = transport
                                .direct_socket_candidates()
                                .await
                                .into_iter()
                                .map(|mut address| {
                                    address.set_port(port);
                                    address
                                })
                                .collect::<Vec<_>>();
                            if advertised.is_empty() {
                                advertised.push(SocketAddr::from(([127, 0, 0, 1], port)));
                            }
                            self.conversation_address_generation = session
                                .conversation_runtime_context()
                                .ok()
                                .and_then(|(_, _, store, _)| {
                                    store.next_conversation_address_generation().ok()
                                })
                                .unwrap_or_else(|| {
                                    self.conversation_address_generation.saturating_add(1)
                                });
                            self.conversation_address_expires_at =
                                unix_seconds().saturating_add(60);
                            if let Some(lookup) = self.conversation_lookup.as_ref() {
                                if let Ok(notice) = lookup.local_address_notice_with_addresses(
                                    self.conversation_address_generation,
                                    self.conversation_address_expires_at,
                                    advertised,
                                ) {
                                    if session
                                        .announce_address_notice(notice.bytes().to_vec())
                                        .is_ok()
                                    {
                                        let _ = transport.flush_session(session).await;
                                    }
                                }
                            }
                        }
                    }
                    changed
                }
                Err(_) => false,
                Ok(Err(error)) => {
                    self.health.network = Some(network_delivery_message(error));
                    self.transport.take();
                    true
                }
            }
        };
        changed |= self.poll_conversation_mesh();
        if changed {
            self.refresh_file_runtime().ok();
        }
        changed
    }

    fn poll_conversation_mesh(&mut self) -> bool {
        let (Some(lookup), Some(conversation)) = (
            self.conversation_lookup.as_mut(),
            self.conversation.as_mut(),
        ) else {
            return false;
        };
        let mut changed = false;
        for _ in 0..MAX_CONVERSATION_MESH_EVENTS_PER_POLL {
            let Some(event) = lookup.mesh().try_event() else {
                break;
            };
            match event {
                ConversationMeshEvent::Received {
                    authenticated_peer,
                    exact_bytes,
                } => {
                    lookup.record_authenticated_observation(authenticated_peer, unix_seconds());
                    let Ok(exact) = ExactRecordV1::decode(&exact_bytes) else {
                        self.health.network =
                            Some("Conversation mesh received malformed bytes.".to_owned());
                        continue;
                    };
                    let result = match exact.record() {
                        ConversationRecordV1::Epoch(_) => conversation
                            .accept_epoch_record(exact.bytes())
                            .map(|()| vec![*exact.id()]),
                        ConversationRecordV1::Channel(_) => conversation
                            .accept_channel_record(exact.bytes())
                            .and_then(|accepted| {
                                accepted.then_some(vec![*exact.id()]).ok_or(
                                    crate::conversations::runtime::ConversationRuntimeError::InvalidEpoch(
                                        "channel record lost deterministic replay",
                                    ),
                                )
                            }),
                        ConversationRecordV1::Message(_) => conversation
                            .accept_message_record(exact.bytes())
                            .map(|_| vec![*exact.id()]),
                        ConversationRecordV1::Acknowledgement(_) => conversation
                            .accept_acknowledgement(
                                authenticated_peer,
                                exact.bytes(),
                                unix_seconds(),
                            )
                            .map(|()| Vec::new()),
                        ConversationRecordV1::RecoveryRequest(_) => conversation
                            .answer_recovery_request(authenticated_peer, exact.bytes())
                            .and_then(|response| {
                                lookup
                                    .mesh()
                                    .send(Some(authenticated_peer), response.bytes())
                                    .map_err(|_| {
                                        crate::conversations::runtime::ConversationRuntimeError::InvalidEpoch(
                                            "recovery response backpressure",
                                        )
                                    })?;
                                Ok(Vec::new())
                            }),
                        ConversationRecordV1::RecoveryResponse(_) => conversation
                            .accept_recovery_response(authenticated_peer, exact.bytes()),
                        ConversationRecordV1::RecipientKey(_)
                        | ConversationRecordV1::AddressNotice(_) => Err(
                            crate::conversations::runtime::ConversationRuntimeError::InvalidEpoch(
                                "Iroh-only conversation control on Commonware",
                            ),
                        ),
                    };
                    match result {
                        Ok(record_ids) => {
                            if self.health.network.as_deref().is_some_and(|message| {
                                message.starts_with("conversation epoch is invalid")
                            }) {
                                self.health.network = None;
                            }
                            if !record_ids.is_empty() {
                                if let Ok(acknowledgement) =
                                    conversation.acknowledgement(record_ids)
                                {
                                    let _ = lookup
                                        .mesh()
                                        .send(Some(authenticated_peer), acknowledgement.bytes());
                                }
                            }
                            changed = true;
                        }
                        Err(error) => self.health.network = Some(error.to_string()),
                    }
                }
                ConversationMeshEvent::AuthenticatedPeerObserved(peer) => {
                    lookup.record_authenticated_observation(peer, unix_seconds());
                    changed = true;
                }
                ConversationMeshEvent::SendDeferred(_) => {}
                ConversationMeshEvent::Fatal(error) => self.health.network = Some(error),
                ConversationMeshEvent::Started { .. } | ConversationMeshEvent::Stopped => {}
            }
        }
        let _ = lookup.maintain_direct_candidates(unix_seconds());
        let peers = lookup.candidate_peers(unix_seconds());
        for peer in peers {
            if let Ok(records) = conversation.pending_commonware_records_for(peer) {
                for record in records {
                    let _ = lookup.mesh().send(Some(peer), &record);
                }
            }
            if let Ok(request) = conversation.recovery_request() {
                let _ = lookup.mesh().send(Some(peer), request.bytes());
            }
        }
        changed
    }

    pub fn spawn_lifecycle(
        application: std::sync::Arc<Mutex<Self>>,
        on_update: std::sync::Arc<dyn Fn(WorkspaceApplicationUpdate) + Send + Sync>,
        runtime: tokio::runtime::Handle,
    ) -> WorkspaceLifecycleHandle {
        let (cancel, mut cancellation) = oneshot::channel();
        let task = runtime.spawn(async move {
            {
                let mut application = application.lock().await;
                application.restart_transport().await;
            }
            on_update(WorkspaceApplicationUpdate::View);
            let mut last_heartbeat = time::Instant::now();
            loop {
                if application.lock().await.transport.is_none() {
                    {
                        let mut application = application.lock().await;
                        application.restart_transport().await;
                    }
                    on_update(WorkspaceApplicationUpdate::View);
                    tokio::select! {
                        _ = &mut cancellation => break,
                        _ = time::sleep(RETRY_BACKOFF) => continue,
                    }
                }

                let transport_changed = application.lock().await.poll_transport().await;
                let files_changed = application.lock().await.poll_files().await;
                if transport_changed {
                    on_update(WorkspaceApplicationUpdate::View);
                    on_update(WorkspaceApplicationUpdate::Files);
                    on_update(WorkspaceApplicationUpdate::Conversations);
                }
                if files_changed {
                    on_update(WorkspaceApplicationUpdate::Files);
                }
                if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
                    if application.lock().await.send_heartbeat_and_expire().await {
                        on_update(WorkspaceApplicationUpdate::View);
                    }
                    last_heartbeat = time::Instant::now();
                }
                tokio::select! {
                    _ = &mut cancellation => break,
                    _ = time::sleep(Duration::from_millis(10)) => {}
                }
            }
            application.lock().await.shutdown_transport().await;
        });
        WorkspaceLifecycleHandle {
            cancel: Some(cancel),
            task: Some(task),
        }
    }

    pub async fn announce_file_changes(&mut self, operation_ids: Vec<String>) {
        let Some(session) = self.session.as_mut() else {
            return;
        };
        if session.announce_file_history(operation_ids).is_err() {
            self.health.storage_unavailable = true;
            return;
        }
        if let Some(transport) = self.transport.as_ref() {
            if let Some(files) = self.files.as_ref() {
                if let Ok(service) = files.recovery_service() {
                    transport.configure_file_recovery(service).await;
                }
            }
            if let Err(error) = transport.flush_session(session).await {
                self.health.network = Some(network_delivery_message(error));
            }
        }
    }

    pub async fn poll_files(&mut self) -> bool {
        let Some(files) = self.files.as_mut() else {
            return false;
        };
        let root_before = files.root_status();
        let changed_files = files.poll_root_changes().unwrap_or_default() > 0;
        let root_changed = root_before != files.root_status();
        let announcements = files.take_pending_announcements();
        if !announcements.is_empty() {
            self.announce_file_changes(announcements).await;
        }
        changed_files || root_changed
    }

    pub async fn send_heartbeat_and_expire(&mut self) -> bool {
        let now = unix_seconds();
        let (presence_expired, changed) = {
            let Some(session) = self.session.as_mut() else {
                return false;
            };
            let presence_expired = match session.expire_presence(now) {
                Ok(expired) => expired,
                Err(_) => {
                    self.health.storage_unavailable = true;
                    false
                }
            };
            let mut changed = presence_expired;
            if let Some(transport) = self.transport.as_ref() {
                if now.saturating_add(15) >= self.conversation_address_expires_at {
                    let conversation_port = self
                        .conversation_lookup
                        .as_ref()
                        .map(|lookup| lookup.mesh().listen_addr().port());
                    if let Some(port) = conversation_port {
                        let mut advertised = transport
                            .direct_socket_candidates()
                            .await
                            .into_iter()
                            .map(|mut address| {
                                address.set_port(port);
                                address
                            })
                            .collect::<Vec<_>>();
                        if advertised.is_empty() {
                            advertised.push(SocketAddr::from(([127, 0, 0, 1], port)));
                        }
                        self.conversation_address_generation = session
                            .conversation_runtime_context()
                            .ok()
                            .and_then(|(_, _, store, _)| {
                                store.next_conversation_address_generation().ok()
                            })
                            .unwrap_or_else(|| {
                                self.conversation_address_generation.saturating_add(1)
                            });
                        self.conversation_address_expires_at = now.saturating_add(60);
                        if let Some(lookup) = self.conversation_lookup.as_ref() {
                            if let Ok(notice) = lookup.local_address_notice_with_addresses(
                                self.conversation_address_generation,
                                self.conversation_address_expires_at,
                                advertised,
                            ) {
                                let _ = session.announce_address_notice(notice.bytes().to_vec());
                            }
                        }
                    }
                }
                if session.is_ready().unwrap_or(false) {
                    let _ = session.announce_recipient_key();
                }
                if let Err(error) = transport.send_session_heartbeat(session).await {
                    self.health.network = Some(network_delivery_message(error));
                    changed = true;
                }
            }
            (presence_expired, changed)
        };
        if presence_expired {
            // Gossip can retain a live endpoint while losing every topic neighbor. Recreate the
            // control subscription from its bootstrap state so address-notice renewal resumes.
            self.restart_transport().await;
        }
        changed
    }

    pub async fn shutdown_transport(&mut self) {
        if let Some(mut lookup) = self.conversation_lookup.take() {
            let _ = lookup.stop();
        }
        if let Some(mut transport) = self.transport.take() {
            let _ = transport.shutdown().await;
        }
    }

    pub fn transport_active(&self) -> bool {
        self.transport.is_some()
    }

    pub fn bootstrap_hint_available(&self) -> bool {
        self.transport.is_some()
    }

    pub async fn bootstrap_hint(&self) -> Result<String, IrohTransportError> {
        self.transport
            .as_ref()
            .ok_or(IrohTransportError::Bind)?
            .bootstrap_hint()
            .await
    }

    pub fn files_mut(&mut self) -> Option<&mut WorkspaceFileRuntime> {
        self.files.as_mut()
    }
    pub fn files(&self) -> Option<&WorkspaceFileRuntime> {
        self.files.as_ref()
    }
    pub fn network_error(&self) -> Option<&str> {
        match &self.health {
            WorkspaceHealth {
                network: Some(message),
                ..
            } => Some(message),
            _ => None,
        }
    }
}

fn conversation_membership_is_ready(
    identity: &InstallationIdentity,
    workspace: &str,
    membership: &crate::membership_log::MembershipLog,
) -> bool {
    let projection = membership.projection(workspace);
    projection.canonical_head.is_some() && projection.contains(&identity.public_identity())
}

fn network_start_message(error: IrohSessionAdapterError) -> String {
    let message = match error {
        IrohSessionAdapterError::Transport(IrohTransportError::Bootstrap) => {
            "The invite's peer address is invalid."
        }
        IrohSessionAdapterError::Transport(IrohTransportError::Bind) => {
            "Peer networking could not start its local endpoint."
        }
        _ => "Peer networking could not start for this workspace.",
    };
    message.to_owned()
}

fn network_delivery_message(error: IrohSessionAdapterError) -> String {
    let message = match error {
        IrohSessionAdapterError::Transport(IrohTransportError::BroadcastClosed) => {
            "Peer networking closed the workspace channel before it could send a message."
        }
        IrohSessionAdapterError::Transport(IrohTransportError::Broadcast) => {
            "Peer networking could not send a workspace message."
        }
        IrohSessionAdapterError::Transport(IrohTransportError::Receive) => {
            "Peer networking could not receive workspace traffic."
        }
        IrohSessionAdapterError::Session(WorkspaceSessionError::InvalidInviteAdmission(reason)) => {
            return format!("Peer networking rejected this message: {reason}.")
        }
        IrohSessionAdapterError::Session(WorkspaceSessionError::Protocol(_)) => {
            "Peer networking received an invalid workspace message."
        }
        IrohSessionAdapterError::Session(_) => "Peer networking could not apply workspace state.",
        _ => "Peer networking is offline. Local workspace data is still available.",
    };
    message.to_owned()
}

fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().try_into().unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    use tokio::{
        sync::Mutex,
        time::{sleep, Duration},
    };

    use super::{
        conversation_membership_is_ready, WorkspaceApplication, WorkspaceApplicationUpdate,
        WorkspaceHealth,
    };
    use crate::{
        identity::{InMemoryKeyCustody, InstallationIdentity},
        membership_log::{MembershipLog, SignedMembershipOperation},
    };

    #[test]
    fn initializes_without_an_active_workspace_as_onboarding_state() {
        let directory = tempfile::tempdir().expect("directory creates");
        let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
            .expect("identity creates");
        let mut application = WorkspaceApplication::initialize(Ok(identity), directory.path());
        let view = application.view();
        assert_eq!(view.workspace, None);
        assert_eq!(view.health, WorkspaceHealth::default());
    }

    #[test]
    fn invalid_identity_is_retained_as_a_separate_health_domain() {
        let directory = tempfile::tempdir().expect("directory creates");
        let mut application = WorkspaceApplication::initialize(
            Err(crate::identity::IdentityError::StoreUnavailable),
            directory.path(),
        );
        assert!(matches!(
            application.view().health,
            WorkspaceHealth {
                identity: Some(_),
                ..
            }
        ));
    }

    #[test]
    fn conversation_waits_for_local_membership_admission() {
        let workspace = "ab".repeat(32);
        let creator =
            InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![1; 32]))
                .expect("creator identity creates");
        let joiner =
            InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![2; 32]))
                .expect("joiner identity creates");
        let mut membership = MembershipLog::new();
        let genesis = SignedMembershipOperation::genesis(&creator, &workspace, "Ada", 1)
            .expect("genesis signs");
        membership.insert(genesis).expect("genesis inserts");

        assert!(!conversation_membership_is_ready(
            &joiner,
            &workspace,
            &membership
        ));

        let addition = SignedMembershipOperation::add_member(
            &creator,
            &workspace,
            membership
                .projection(&workspace)
                .canonical_head
                .expect("genesis head")
                .as_str(),
            1,
            *joiner.public_identity().as_bytes(),
            "Lin",
            2,
        )
        .expect("member addition signs");
        membership
            .insert(addition)
            .expect("member addition inserts");

        assert!(conversation_membership_is_ready(
            &joiner,
            &workspace,
            &membership
        ));
    }

    #[tokio::test]
    async fn lifecycle_handle_stops_polling_and_shuts_down_cleanly() {
        let directory = tempfile::tempdir().expect("directory creates");
        let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
            .expect("identity creates");
        let application = Arc::new(Mutex::new(WorkspaceApplication::initialize(
            Ok(identity),
            directory.path(),
        )));
        let updates = Arc::new(AtomicUsize::new(0));
        let callback_updates = Arc::clone(&updates);
        let callback = Arc::new(move |_: WorkspaceApplicationUpdate| {
            callback_updates.fetch_add(1, Ordering::Relaxed);
        });
        let mut lifecycle = WorkspaceApplication::spawn_lifecycle(
            Arc::clone(&application),
            callback,
            tokio::runtime::Handle::current(),
        );
        sleep(Duration::from_millis(20)).await;
        lifecycle.stop().await;
        assert!(updates.load(Ordering::Relaxed) >= 1);
        assert!(!application.lock().await.transport_active());
    }

    #[test]
    fn lifecycle_can_start_from_a_non_async_caller() {
        let runtime = tokio::runtime::Runtime::new().expect("runtime creates");
        let runtime_handle = runtime.handle().clone();
        let directory = tempfile::tempdir().expect("directory creates");
        let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
            .expect("identity creates");
        let application = Arc::new(Mutex::new(WorkspaceApplication::initialize(
            Ok(identity),
            directory.path(),
        )));
        let updates = Arc::new(AtomicUsize::new(0));
        let callback_updates = Arc::clone(&updates);
        let callback = Arc::new(move |_: WorkspaceApplicationUpdate| {
            callback_updates.fetch_add(1, Ordering::Relaxed);
        });

        let mut lifecycle = WorkspaceApplication::spawn_lifecycle(
            Arc::clone(&application),
            callback,
            runtime_handle,
        );
        runtime.block_on(async {
            sleep(Duration::from_millis(20)).await;
            lifecycle.stop().await;
            assert!(!application.lock().await.transport_active());
        });

        assert!(updates.load(Ordering::Relaxed) >= 1);
    }
}
