//! Runtime-owned application service for the active workspace.
//!
//! This module owns session, transport, file-runtime, and health state. The
//! desktop crate translates its views and errors into Tauri wire contracts.

use std::{
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
    identity::{IdentityError, InstallationIdentity, PublicIdentity},
    iroh_transport::{IrohSessionAdapterError, IrohTransport, IrohTransportError},
    workspace_catalog::WorkspaceCatalog,
    workspace_domain::{KnownPeer, Member, WorkspaceSummary},
    workspace_file_runtime::WorkspaceFileRuntime,
    workspace_session::{
        FakeDeliveryPort, WorkspaceSession, WorkspaceSessionError, WorkspaceTransition,
    },
};

pub const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
pub const TRANSPORT_POLL_INTERVAL: Duration = Duration::from_millis(500);
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceApplicationUpdate {
    View,
    Files,
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
        let (session, files, local_public_identity, health) = match identity {
            Ok(identity) => match WorkspaceCatalog::open(application_data) {
                Ok(catalog) => {
                    let mut session =
                        WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
                    let local_public_identity = session.local_public_identity_value();
                    match session.activate_active_workspace() {
                        Ok(_) => {
                            let files = session.open_file_runtime().ok();
                            (
                                Some(session),
                                files,
                                Some(local_public_identity),
                                WorkspaceHealth::default(),
                            )
                        }
                        Err(_) => (None, None, None, WorkspaceHealth::storage_error()),
                    }
                }
                Err(_) => (None, None, None, WorkspaceHealth::storage_error()),
            },
            Err(error) => (
                None,
                None,
                None,
                WorkspaceHealth::identity_error(error.to_string()),
            ),
        };
        Self {
            session,
            transport: None,
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
        self.health = WorkspaceHealth::default();
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
        Ok(())
    }

    pub fn retry_join(&mut self, display_name: String) -> Result<bool, WorkspaceSessionError> {
        self.session
            .as_mut()
            .ok_or(WorkspaceSessionError::NoActiveWorkspace)?
            .retry_join(display_name)
    }

    pub fn refresh_file_runtime(&mut self) -> Result<(), WorkspaceSessionError> {
        self.files = self
            .session
            .as_ref()
            .and_then(|session| session.open_file_runtime().ok());
        Ok(())
    }

    pub async fn restart_transport(&mut self) {
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
                if let Err(error) = active.flush_session(session).await {
                    self.health.network = Some(network_delivery_message(error));
                } else {
                    self.health.network = None;
                }
                self.transport = Some(active);
            }
            Err(error) => self.health.network = Some(network_start_message(error)),
        }
    }

    pub async fn poll_transport(&mut self) -> bool {
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
                if changed {
                    self.refresh_file_runtime().ok();
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
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        let mut changed = match session.expire_presence(now) {
            Ok(changed) => changed,
            Err(_) => {
                self.health.storage_unavailable = true;
                true
            }
        };
        if let Some(transport) = self.transport.as_ref() {
            if let Err(error) = transport.send_session_heartbeat(session).await {
                self.health.network = Some(network_delivery_message(error));
                changed = true;
            }
        }
        changed
    }

    pub async fn shutdown_transport(&mut self) {
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

    use super::{WorkspaceApplication, WorkspaceApplicationUpdate, WorkspaceHealth};
    use crate::identity::{InMemoryKeyCustody, InstallationIdentity};

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
