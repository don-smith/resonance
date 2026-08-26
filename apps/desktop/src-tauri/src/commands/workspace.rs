use std::sync::Arc;

use resonance_runtime::{
    identity::{IdentityError, InstallationIdentity},
    invite::Invite,
    workspace_application::{
        WorkspaceApplication, WorkspaceApplicationUpdate, WorkspaceApplicationView,
        WorkspaceHealth, WorkspaceLifecycleHandle,
    },
    workspace_domain::{KnownPeer, Member, PeerConnection, WorkspaceLifecycle, WorkspaceSummary},
    workspace_session::WorkspaceTransition,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Mutex;

const MAX_DISPLAY_NAME_LENGTH: usize = 255;
const MAX_INVITE_LENGTH: usize = 16_384;
const MAX_RELAY_LENGTH: usize = 4_096;

pub struct ManagedWorkspaceState {
    pub(super) inner: Arc<ManagedWorkspace>,
}

pub(super) struct ManagedWorkspace {
    app: AppHandle,
    pub(super) application: Arc<Mutex<WorkspaceApplication>>,
    lifecycle: Mutex<Option<WorkspaceLifecycleHandle>>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceShellView {
    pub revision: u64,
    pub state: ShellState,
    pub message: Option<String>,
    pub health: ShellHealthView,
    pub workspace: Option<WorkspaceView>,
    pub local_public_identity: Option<String>,
    pub members: Vec<MemberView>,
    pub peers: Vec<PeerView>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellHealthView {
    pub identity: HealthState,
    pub storage: HealthState,
    pub network: HealthState,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ShellCommandError {
    pub code: ShellErrorCode,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellErrorCode {
    InvalidRequest,
    InvalidInvite,
    NetworkOffline,
    StorageUnavailable,
    Internal,
}

impl ShellCommandError {
    fn new(code: ShellErrorCode) -> Self {
        let message = match code {
            ShellErrorCode::InvalidRequest => "The workspace request is invalid.",
            ShellErrorCode::InvalidInvite => "That invite is invalid or has been altered.",
            ShellErrorCode::NetworkOffline => "Peer networking is not ready yet. Try again.",
            ShellErrorCode::StorageUnavailable => {
                "The installation identity or workspace storage is unavailable."
            }
            ShellErrorCode::Internal => "Resonance could not complete the workspace request.",
        };
        Self {
            code,
            message: message.to_owned(),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum HealthState {
    Healthy,
    Unavailable,
    Offline,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellState {
    Onboarding,
    Initializing,
    Ready,
    Joining,
    IdentityError,
    StorageError,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceView {
    pub id: String,
    pub display_name: String,
    pub lifecycle: WorkspaceLifecycleView,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceLifecycleView {
    Initializing,
    Ready,
    Joining,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemberView {
    pub public_identity: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PeerView {
    pub public_identity: String,
    pub display_name: String,
    pub online: bool,
    pub connection: PeerConnectionView,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PeerConnectionView {
    Direct,
    Relayed,
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspaceRequest {
    pub display_name: String,
    pub creator_display_name: String,
    pub relay_override: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JoinWorkspaceRequest {
    pub invite: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RetryJoinRequest {
    pub display_name: String,
}

impl CreateWorkspaceRequest {
    fn validate(&self) -> bool {
        valid_text(&self.display_name, MAX_DISPLAY_NAME_LENGTH)
            && valid_text(&self.creator_display_name, MAX_DISPLAY_NAME_LENGTH)
            && self
                .relay_override
                .as_ref()
                .is_none_or(|value| valid_text(value, MAX_RELAY_LENGTH))
    }
}

impl JoinWorkspaceRequest {
    fn validate(&self) -> bool {
        valid_text(&self.invite, MAX_INVITE_LENGTH)
            && valid_text(&self.display_name, MAX_DISPLAY_NAME_LENGTH)
    }
}

impl RetryJoinRequest {
    fn validate(&self) -> bool {
        valid_text(&self.display_name, MAX_DISPLAY_NAME_LENGTH)
    }
}

fn valid_text(value: &str, max_length: usize) -> bool {
    !value.trim().is_empty() && value.chars().count() <= max_length
}

impl ManagedWorkspaceState {
    pub(crate) async fn shutdown(&self) {
        self.inner.shutdown().await;
    }

    pub fn initialize(
        app: AppHandle,
        identity: Result<InstallationIdentity, IdentityError>,
        application_data: &std::path::Path,
    ) -> Self {
        Self {
            inner: Arc::new(ManagedWorkspace {
                app,
                application: Arc::new(Mutex::new(WorkspaceApplication::initialize(
                    identity,
                    application_data,
                ))),
                lifecycle: Mutex::new(None),
            }),
        }
    }

    pub fn start_lifecycle(&self) {
        let workspace = Arc::clone(&self.inner);
        let application = Arc::clone(&workspace.application);
        let weak_workspace = Arc::downgrade(&workspace);
        let on_update = Arc::new(move |update: WorkspaceApplicationUpdate| {
            let Some(workspace) = weak_workspace.upgrade() else {
                return;
            };
            tauri::async_runtime::spawn(async move {
                match update {
                    WorkspaceApplicationUpdate::View => workspace.emit_view().await,
                    WorkspaceApplicationUpdate::Files => workspace.emit_files_changed(),
                }
            });
        });
        let lifecycle = WorkspaceApplication::spawn_lifecycle(application, on_update);
        tauri::async_runtime::spawn(async move {
            *workspace.lifecycle.lock().await = Some(lifecycle);
        });
    }
}

impl ManagedWorkspace {
    pub(super) async fn view(&self) -> WorkspaceShellView {
        let mut application = self.application.lock().await;
        shell_view(application.view())
    }

    pub(super) async fn emit_view(&self) {
        let (view, transitions) = {
            let mut application = self.application.lock().await;
            let view = shell_view(application.view());
            let transitions = application.take_transitions();
            (view, transitions)
        };
        let _ = self.app.emit("workspace:changed", view);
        for transition in transitions {
            match transition {
                WorkspaceTransition::WorkspaceChanged(_) => {}
                WorkspaceTransition::MemberJoined(member) => {
                    let _ = self
                        .app
                        .emit("workspace:member-added", member_view(&member));
                }
                WorkspaceTransition::PeerPresenceChanged(peer) => {
                    let peer = peer_view(&peer, &[]);
                    let _ = self.app.emit("peer:connection", peer.clone());
                    let event = if peer.online {
                        "peer:joined"
                    } else {
                        "peer:left"
                    };
                    let _ = self.app.emit(event, peer);
                }
            }
        }
    }

    pub(super) fn emit_files_changed(&self) {
        let _ = self.app.emit("workspace-files:changed", ());
    }

    pub(super) async fn refresh_file_runtime(&self) {
        let mut application = self.application.lock().await;
        let _ = application.refresh_file_runtime();
    }

    pub(super) async fn announce_file_changes(&self, operation_ids: Vec<String>) {
        let mut application = self.application.lock().await;
        application.announce_file_changes(operation_ids).await;
    }

    pub(super) async fn restart_transport(&self) {
        let mut application = self.application.lock().await;
        application.restart_transport().await;
    }

    pub(super) async fn shutdown(&self) {
        if let Some(mut lifecycle) = self.lifecycle.lock().await.take() {
            lifecycle.stop().await;
        } else {
            self.application.lock().await.shutdown_transport().await;
        }
    }

    pub(super) async fn with_application<T>(
        &self,
        operation: impl FnOnce(&mut WorkspaceApplication) -> T,
    ) -> T {
        let mut application = self.application.lock().await;
        operation(&mut application)
    }

    pub(super) async fn with_files<T>(
        &self,
        operation: impl FnOnce(
            Option<&mut resonance_runtime::workspace_file_runtime::WorkspaceFileRuntime>,
        ) -> T,
    ) -> T {
        let mut application = self.application.lock().await;
        operation(application.files_mut())
    }

    pub(super) async fn bootstrap_hint(
        &self,
    ) -> Result<String, resonance_runtime::iroh_transport::IrohTransportError> {
        let application = self.application.lock().await;
        application.bootstrap_hint().await
    }
}

#[tauri::command]
pub async fn workspace_view(
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, ShellCommandError> {
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn create_workspace(
    request: CreateWorkspaceRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, ShellCommandError> {
    if !request.validate() {
        return Err(ShellCommandError::new(ShellErrorCode::InvalidRequest));
    }
    state
        .inner
        .with_application(|application| {
            application.create_workspace(
                request.display_name,
                request.creator_display_name,
                request.relay_override,
            )
        })
        .await
        .map_err(|_| ShellCommandError::new(ShellErrorCode::StorageUnavailable))?;
    state.inner.restart_transport().await;
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn create_workspace_invite(
    state: State<'_, ManagedWorkspaceState>,
) -> Result<String, ShellCommandError> {
    let bootstrap = state
        .inner
        .bootstrap_hint()
        .await
        .map_err(|_| ShellCommandError::new(ShellErrorCode::NetworkOffline))?;
    state
        .inner
        .with_application(|application| application.create_invite(bootstrap))
        .await
        .map_err(|_| ShellCommandError::new(ShellErrorCode::Internal))
}

#[tauri::command]
pub async fn join_workspace(
    request: JoinWorkspaceRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, ShellCommandError> {
    if !request.validate() {
        return Err(ShellCommandError::new(ShellErrorCode::InvalidRequest));
    }
    Invite::decode(&request.invite)
        .map_err(|_| ShellCommandError::new(ShellErrorCode::InvalidInvite))?;
    state
        .inner
        .with_application(|application| {
            application.join_workspace(&request.invite, request.display_name)
        })
        .await
        .map_err(|_| ShellCommandError::new(ShellErrorCode::Internal))?;
    state.inner.restart_transport().await;
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn retry_workspace_join(
    request: RetryJoinRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, ShellCommandError> {
    if !request.validate() {
        return Err(ShellCommandError::new(ShellErrorCode::InvalidRequest));
    }
    let retry_sent = state
        .inner
        .with_application(|application| application.retry_join(request.display_name))
        .await
        .map_err(|_| ShellCommandError::new(ShellErrorCode::Internal))?;
    if retry_sent {
        state.inner.restart_transport().await;
        state.inner.emit_view().await;
    }
    state.inner.refresh_file_runtime().await;
    Ok(state.inner.view().await)
}

fn shell_view(view: WorkspaceApplicationView) -> WorkspaceShellView {
    let state = if view.health.identity.is_some() {
        ShellState::IdentityError
    } else if view.health.storage_unavailable {
        ShellState::StorageError
    } else if let Some(workspace) = view.workspace.as_ref() {
        match workspace.lifecycle {
            WorkspaceLifecycle::Initializing => ShellState::Initializing,
            WorkspaceLifecycle::Ready => ShellState::Ready,
            WorkspaceLifecycle::Joining => ShellState::Joining,
        }
    } else {
        ShellState::Onboarding
    };
    let message = view
        .health
        .identity
        .clone()
        .or_else(|| {
            view.health
                .storage_unavailable
                .then_some("Resonance cannot open its local workspace data.".to_owned())
        })
        .or_else(|| view.health.network.clone());
    WorkspaceShellView {
        revision: view.revision,
        state,
        message,
        health: health_view(&view.health),
        workspace: view.workspace.as_ref().map(workspace_summary_view),
        local_public_identity: view
            .local_public_identity
            .map(|identity| identity.to_string()),
        members: view.members.iter().map(member_view).collect(),
        peers: view
            .peers
            .iter()
            .map(|peer| peer_view(peer, &view.members))
            .collect(),
    }
}

fn health_view(health: &WorkspaceHealth) -> ShellHealthView {
    ShellHealthView {
        identity: if health.identity.is_some() {
            HealthState::Unavailable
        } else {
            HealthState::Healthy
        },
        storage: if health.storage_unavailable {
            HealthState::Unavailable
        } else {
            HealthState::Healthy
        },
        network: if health.network.is_some() {
            HealthState::Offline
        } else {
            HealthState::Healthy
        },
    }
}

fn workspace_summary_view(workspace: &WorkspaceSummary) -> WorkspaceView {
    WorkspaceView {
        id: workspace.id.as_str().to_owned(),
        display_name: workspace.display_name.clone(),
        lifecycle: match workspace.lifecycle {
            WorkspaceLifecycle::Initializing => WorkspaceLifecycleView::Initializing,
            WorkspaceLifecycle::Ready => WorkspaceLifecycleView::Ready,
            WorkspaceLifecycle::Joining => WorkspaceLifecycleView::Joining,
        },
    }
}

fn member_view(member: &Member) -> MemberView {
    MemberView {
        public_identity: member.public_identity.to_string(),
        display_name: member.display_name.clone(),
        role: member.role.clone(),
    }
}

fn peer_view(peer: &KnownPeer, members: &[Member]) -> PeerView {
    let display_name = members
        .iter()
        .find(|member| member.public_identity == peer.public_identity)
        .map(|member| member.display_name.clone())
        .unwrap_or_else(|| {
            let identity = peer.public_identity.to_string();
            identity[..12.min(identity.len())].to_owned()
        });
    PeerView {
        public_identity: peer.public_identity.to_string(),
        display_name,
        online: peer.online,
        connection: match peer.connection {
            PeerConnection::Direct => PeerConnectionView::Direct,
            PeerConnection::Relayed => PeerConnectionView::Relayed,
            PeerConnection::Unknown => PeerConnectionView::Unknown,
        },
    }
}
