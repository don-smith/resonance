use std::{path::Path, sync::Arc, time::Duration};

use resonance_runtime::{
    identity::InstallationIdentity,
    invite::Invite,
    iroh_transport::{IrohSessionAdapterError, IrohTransport, IrohTransportError},
    workspace_catalog::WorkspaceCatalog,
    workspace_domain::{KnownPeer, Member, PeerConnection, WorkspaceLifecycle, WorkspaceSummary},
    workspace_session::{FakeDeliveryPort, WorkspaceSession, WorkspaceTransition},
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::{sync::Mutex, time};

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const TRANSPORT_POLL_INTERVAL: Duration = Duration::from_millis(500);

pub struct ManagedWorkspaceState {
    inner: Arc<ManagedWorkspace>,
}

struct ManagedWorkspace {
    app: AppHandle,
    session: Mutex<Option<WorkspaceSession<FakeDeliveryPort>>>,
    transport: Mutex<Option<IrohTransport>>,
    bootstrap: Mutex<Option<String>>,
    issue: Mutex<Option<WorkspaceIssue>>,
    local_public_identity: Option<String>,
}

#[derive(Clone, Debug)]
enum WorkspaceIssue {
    Identity(String),
    Storage,
    Network(String),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceShellView {
    pub state: String,
    pub message: Option<String>,
    pub workspace: Option<WorkspaceView>,
    pub local_public_identity: Option<String>,
    pub members: Vec<MemberView>,
    pub peers: Vec<PeerView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub id: String,
    pub display_name: String,
    pub lifecycle: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberView {
    pub public_identity: String,
    pub display_name: String,
    pub role: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeerView {
    pub public_identity: String,
    pub online: bool,
    pub connection: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspaceRequest {
    pub display_name: String,
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

impl ManagedWorkspaceState {
    pub fn initialize(
        app: AppHandle,
        identity: Result<InstallationIdentity, resonance_runtime::identity::IdentityError>,
        application_data: &Path,
    ) -> Self {
        let (session, local_public_identity, issue) = match identity {
            Ok(identity) => match WorkspaceCatalog::open(application_data) {
                Ok(catalog) => {
                    let mut session =
                        WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
                    let local_public_identity = session.local_public_identity();
                    match session.activate_active_workspace() {
                        Ok(_) => (Some(session), Some(local_public_identity), None),
                        Err(_) => (None, None, Some(WorkspaceIssue::Storage)),
                    }
                }
                Err(_) => (None, None, Some(WorkspaceIssue::Storage)),
            },
            Err(error) => (
                None,
                None,
                Some(WorkspaceIssue::Identity(error.to_string())),
            ),
        };
        Self {
            inner: Arc::new(ManagedWorkspace {
                app,
                session: Mutex::new(session),
                transport: Mutex::new(None),
                bootstrap: Mutex::new(None),
                issue: Mutex::new(issue),
                local_public_identity,
            }),
        }
    }

    pub fn start_lifecycle(&self) {
        let workspace = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            workspace.restart_transport(None).await;
            workspace.emit_view().await;
            let mut last_heartbeat = time::Instant::now();
            loop {
                workspace.poll_transport().await;
                if last_heartbeat.elapsed() >= HEARTBEAT_INTERVAL {
                    workspace.send_heartbeat_and_expire().await;
                    last_heartbeat = time::Instant::now();
                }
            }
        });
    }
}

impl ManagedWorkspace {
    async fn view(&self) -> WorkspaceShellView {
        let mut session = self.session.lock().await;
        let issue = self.issue.lock().await.clone();
        let Some(session) = session.as_mut() else {
            return WorkspaceShellView {
                state: match issue {
                    Some(WorkspaceIssue::Identity(_)) => "identity-error".to_owned(),
                    _ => "storage-error".to_owned(),
                },
                message: issue.map(issue_message),
                workspace: None,
                local_public_identity: None,
                members: Vec::new(),
                peers: Vec::new(),
            };
        };
        if !session.has_active_workspace() {
            return WorkspaceShellView {
                state: "onboarding".to_owned(),
                message: None,
                workspace: None,
                local_public_identity: self.local_public_identity.clone(),
                members: Vec::new(),
                peers: Vec::new(),
            };
        }
        match session.view() {
            Ok(view) => WorkspaceShellView {
                state: match view.workspace.lifecycle {
                    WorkspaceLifecycle::Ready => "ready".to_owned(),
                    WorkspaceLifecycle::Joining => "joining".to_owned(),
                },
                message: issue.and_then(|issue| match issue {
                    WorkspaceIssue::Network(_) => Some(issue_message(issue)),
                    WorkspaceIssue::Identity(_) | WorkspaceIssue::Storage => None,
                }),
                workspace: Some(workspace_summary_view(&view.workspace)),
                local_public_identity: Some(view.local_public_identity),
                members: view.members.iter().map(member_view).collect(),
                peers: view.peers.iter().map(peer_view).collect(),
            },
            Err(_) => WorkspaceShellView {
                state: "storage-error".to_owned(),
                message: Some(issue_message(WorkspaceIssue::Storage)),
                workspace: None,
                local_public_identity: self.local_public_identity.clone(),
                members: Vec::new(),
                peers: Vec::new(),
            },
        }
    }

    async fn emit_view(&self) {
        let view = self.view().await;
        let transitions = {
            let mut session = self.session.lock().await;
            session
                .as_mut()
                .map(WorkspaceSession::take_transitions)
                .unwrap_or_default()
        };
        let _ = self.app.emit("workspace:changed", view);
        for transition in transitions {
            match transition {
                WorkspaceTransition::WorkspaceChanged(_) => {}
                WorkspaceTransition::MemberJoined(member) => {
                    let _ = self
                        .app
                        .emit("workspace:member-joined", member_view(&member));
                }
                WorkspaceTransition::PeerPresenceChanged(peer) => {
                    let peer = peer_view(&peer);
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

    async fn restart_transport(&self, bootstrap: Option<String>) {
        let mut transport = self.transport.lock().await;
        if let Some(mut active) = transport.take() {
            let _ = active.shutdown().await;
        }
        let mut session = self.session.lock().await;
        let Some(session) = session.as_mut() else {
            return;
        };
        match IrohTransport::start_for_session(session, bootstrap.as_deref()).await {
            Ok(active) => {
                if active.flush_session(session).await.is_err() {
                    *self.issue.lock().await = Some(network_issue());
                } else {
                    *self.issue.lock().await = None;
                }
                *self.bootstrap.lock().await = bootstrap;
                *transport = Some(active);
            }
            Err(error) => *self.issue.lock().await = Some(network_start_issue(error)),
        }
    }

    async fn poll_transport(&self) {
        let mut transport_guard = self.transport.lock().await;
        let mut session_guard = self.session.lock().await;
        let (has_transport, should_emit) = if let (Some(transport), Some(session)) =
            (transport_guard.as_mut(), session_guard.as_mut())
        {
            let result = time::timeout(
                TRANSPORT_POLL_INTERVAL,
                transport.apply_next_session_event(session),
            )
            .await;
            let should_emit = match result {
                Ok(Ok(true)) => {
                    if transport.flush_session(session).await.is_err() {
                        *self.issue.lock().await = Some(network_issue());
                    }
                    true
                }
                Ok(Ok(false)) | Err(_) => false,
                Ok(Err(_)) => {
                    *self.issue.lock().await = Some(network_issue());
                    true
                }
            };
            (true, should_emit)
        } else {
            (false, false)
        };
        drop(session_guard);
        drop(transport_guard);
        if should_emit {
            self.emit_view().await;
        } else if !has_transport {
            time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn send_heartbeat_and_expire(&self) {
        let now = unix_seconds();
        {
            let mut session = self.session.lock().await;
            let Some(session) = session.as_mut() else {
                return;
            };
            if !session.has_active_workspace() {
                return;
            }
            if session.expire_presence(now).is_err() {
                *self.issue.lock().await = Some(WorkspaceIssue::Storage);
            }
        }
        let transport = self.transport.lock().await;
        let mut session = self.session.lock().await;
        if let (Some(transport), Some(session)) = (transport.as_ref(), session.as_mut()) {
            if transport.send_session_heartbeat(session).await.is_err() {
                *self.issue.lock().await = Some(network_issue());
            }
        }
        drop(session);
        drop(transport);
        self.emit_view().await;
    }
}

#[tauri::command]
pub async fn workspace_view(
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn create_workspace(
    request: CreateWorkspaceRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    {
        let mut session = state.inner.session.lock().await;
        let session = session.as_mut().ok_or_else(|| {
            "The installation identity or workspace storage is unavailable.".to_owned()
        })?;
        session
            .create_workspace(request.display_name, request.relay_override)
            .map_err(|_| "Resonance could not create this workspace.".to_owned())?;
    }
    state.inner.restart_transport(None).await;
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn create_workspace_invite(
    state: State<'_, ManagedWorkspaceState>,
) -> Result<String, String> {
    let bootstrap = {
        let transport = state.inner.transport.lock().await;
        let transport = transport
            .as_ref()
            .ok_or_else(|| "Peer networking is offline. Retry after it reconnects.".to_owned())?;
        transport
            .bootstrap_hint()
            .await
            .map_err(|_| "Peer networking is not ready yet. Try again.".to_owned())?
    };
    let session = state.inner.session.lock().await;
    session
        .as_ref()
        .ok_or_else(|| "The installation identity or workspace storage is unavailable.".to_owned())?
        .create_invite(bootstrap)
        .map_err(|_| "Resonance could not create an invite for this workspace.".to_owned())
}

#[tauri::command]
pub async fn join_workspace(
    request: JoinWorkspaceRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    let bootstrap = Invite::decode(&request.invite)
        .map_err(|_| "That invite is invalid or has been altered.".to_owned())?
        .bootstrap()
        .to_owned();
    {
        let mut session = state.inner.session.lock().await;
        let session = session.as_mut().ok_or_else(|| {
            "The installation identity or workspace storage is unavailable.".to_owned()
        })?;
        session
            .join_workspace(&request.invite, request.display_name)
            .map_err(|_| "Resonance could not join this workspace.".to_owned())?;
    }
    state.inner.restart_transport(Some(bootstrap)).await;
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn retry_workspace_join(
    request: RetryJoinRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    {
        let mut session = state.inner.session.lock().await;
        let session = session.as_mut().ok_or_else(|| {
            "The installation identity or workspace storage is unavailable.".to_owned()
        })?;
        session
            .retry_join(request.display_name)
            .map_err(|_| "This workspace is not waiting for invite admission.".to_owned())?;
    }
    let bootstrap = state.inner.bootstrap.lock().await.clone();
    state.inner.restart_transport(bootstrap).await;
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

fn workspace_summary_view(workspace: &WorkspaceSummary) -> WorkspaceView {
    WorkspaceView {
        id: workspace.id.as_str().to_owned(),
        display_name: workspace.display_name.clone(),
        lifecycle: match workspace.lifecycle {
            WorkspaceLifecycle::Ready => "ready".to_owned(),
            WorkspaceLifecycle::Joining => "joining".to_owned(),
        },
    }
}

fn member_view(member: &Member) -> MemberView {
    MemberView {
        public_identity: member.public_identity.clone(),
        display_name: member.display_name.clone(),
        role: member.role.clone(),
    }
}

fn peer_view(peer: &KnownPeer) -> PeerView {
    PeerView {
        public_identity: peer.public_identity.clone(),
        online: peer.online,
        connection: match peer.connection {
            PeerConnection::Direct => "direct".to_owned(),
            PeerConnection::Relayed => "relayed".to_owned(),
            PeerConnection::Unknown => "unknown".to_owned(),
        },
    }
}

fn issue_message(issue: WorkspaceIssue) -> String {
    match issue {
        WorkspaceIssue::Identity(message) => message,
        WorkspaceIssue::Storage => "Resonance cannot open its local workspace data.".to_owned(),
        WorkspaceIssue::Network(message) => message,
    }
}

fn network_issue() -> WorkspaceIssue {
    WorkspaceIssue::Network(
        "Peer networking is offline. Local workspace data is still available.".to_owned(),
    )
}

fn network_start_issue(error: IrohSessionAdapterError) -> WorkspaceIssue {
    let message = match error {
        IrohSessionAdapterError::Transport(IrohTransportError::Bind) => {
            "Peer networking could not start its local endpoint."
        }
        IrohSessionAdapterError::Transport(IrohTransportError::Bootstrap) => {
            "The invite's peer address is invalid."
        }
        IrohSessionAdapterError::Transport(IrohTransportError::Subscribe) => {
            "The workspace peer channel could not start."
        }
        IrohSessionAdapterError::Transport(
            IrohTransportError::Broadcast
            | IrohTransportError::Receive
            | IrohTransportError::Shutdown,
        )
        | IrohSessionAdapterError::Session(_) => {
            "Peer networking could not start for this workspace."
        }
    };
    WorkspaceIssue::Network(message.to_owned())
}

fn unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs().try_into().unwrap_or(i64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        issue_message, network_start_issue, IrohSessionAdapterError, IrohTransportError,
        MemberView, PeerView, WorkspaceShellView, WorkspaceView,
    };

    #[test]
    fn maps_transport_start_errors_to_safe_actionable_messages() {
        let message = issue_message(network_start_issue(IrohSessionAdapterError::Transport(
            IrohTransportError::Bootstrap,
        )));
        assert_eq!(message, "The invite's peer address is invalid.");
        for forbidden in ["secret", "token", "bootstrap", "path", "iroh"] {
            assert!(!message.to_ascii_lowercase().contains(forbidden));
        }
    }

    #[test]
    fn public_workspace_event_contains_no_secret_or_transport_fields() {
        let view = WorkspaceShellView {
            state: "ready".to_owned(),
            message: None,
            workspace: Some(WorkspaceView {
                id: "opaque-id".to_owned(),
                display_name: "Team".to_owned(),
                lifecycle: "ready".to_owned(),
            }),
            local_public_identity: Some("public-id".to_owned()),
            members: vec![MemberView {
                public_identity: "member-id".to_owned(),
                display_name: "Ada".to_owned(),
                role: "developer".to_owned(),
            }],
            peers: vec![PeerView {
                public_identity: "member-id".to_owned(),
                online: true,
                connection: "direct".to_owned(),
            }],
        };

        let payload = serde_json::to_string(&view).expect("view serializes");
        for forbidden in ["secret", "token", "private", "bootstrap", "path", "iroh"] {
            assert!(!payload.to_ascii_lowercase().contains(forbidden));
        }
    }
}
