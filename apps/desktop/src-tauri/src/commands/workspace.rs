use std::{path::Path, sync::Arc, time::Duration};

use resonance_runtime::{
    identity::InstallationIdentity,
    invite::Invite,
    iroh_transport::{IrohSessionAdapterError, IrohTransport, IrohTransportError},
    local_root_binding::{RootHealth, RootSelection},
    workspace_catalog::WorkspaceCatalog,
    workspace_domain::{KnownPeer, Member, PeerConnection, WorkspaceLifecycle, WorkspaceSummary},
    workspace_file_runtime::{
        FileConflictView, FileEntryKind, FileTreeEntry, MarkdownFileView, RootBindingStatus,
        WorkspaceFileRuntime,
    },
    workspace_files::projection::ConflictKind,
    workspace_session::{
        FakeDeliveryPort, WorkspaceSession, WorkspaceSessionError, WorkspaceTransition,
    },
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
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
    files: Mutex<Option<WorkspaceFileRuntime>>,
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
    pub files: Option<WorkspaceFilesView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceFilesView {
    pub root: RootView,
    pub entries: Vec<FileEntryView>,
    pub conflicts: Vec<ConflictView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RootView {
    pub state: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntryView {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub name: String,
    pub kind: String,
    pub current_revision_id: Option<String>,
    pub editable: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictView {
    pub record_id: String,
    pub node_id: String,
    pub kind: String,
    pub competing_revision_ids: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownRevisionView {
    pub node_id: String,
    pub revision_id: String,
    pub markdown: String,
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
    pub display_name: String,
    pub online: bool,
    pub connection: String,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenMarkdownRequest {
    pub node_id: String,
    pub revision_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateMarkdownRequest {
    pub parent_node_id: String,
    pub name: String,
    pub markdown: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceMarkdownRequest {
    pub node_id: String,
    pub base_revision_id: String,
    pub markdown: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveConflictRequest {
    pub record_id: String,
    pub chosen_revision_id: Option<String>,
}

impl ManagedWorkspaceState {
    pub fn initialize(
        app: AppHandle,
        identity: Result<InstallationIdentity, resonance_runtime::identity::IdentityError>,
        application_data: &Path,
    ) -> Self {
        let (session, files, local_public_identity, issue) = match identity {
            Ok(identity) => match WorkspaceCatalog::open(application_data) {
                Ok(catalog) => {
                    let mut session =
                        WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
                    let local_public_identity = session.local_public_identity();
                    match session.activate_active_workspace() {
                        Ok(_) => {
                            let files = session.open_file_runtime().ok();
                            (Some(session), files, Some(local_public_identity), None)
                        }
                        Err(_) => (None, None, None, Some(WorkspaceIssue::Storage)),
                    }
                }
                Err(_) => (None, None, None, Some(WorkspaceIssue::Storage)),
            },
            Err(error) => (
                None,
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
                files: Mutex::new(files),
                issue: Mutex::new(issue),
                local_public_identity,
            }),
        }
    }

    pub fn start_lifecycle(&self) {
        let workspace = Arc::clone(&self.inner);
        tauri::async_runtime::spawn(async move {
            workspace.restart_transport().await;
            workspace.emit_view().await;
            let mut last_heartbeat = time::Instant::now();
            loop {
                workspace.poll_transport().await;
                workspace.poll_files().await;
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
        let files = self.files.lock().await.as_ref().map(workspace_files_view);
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
                files: None,
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
                files: None,
            };
        }
        match session.view() {
            Ok(view) => WorkspaceShellView {
                state: match view.workspace.lifecycle {
                    WorkspaceLifecycle::Initializing => "initializing".to_owned(),
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
                peers: view
                    .peers
                    .iter()
                    .map(|peer| peer_view(peer, &view.members))
                    .collect(),
                files,
            },
            Err(_) => WorkspaceShellView {
                state: "storage-error".to_owned(),
                message: Some(issue_message(WorkspaceIssue::Storage)),
                workspace: None,
                local_public_identity: self.local_public_identity.clone(),
                members: Vec::new(),
                peers: Vec::new(),
                files: None,
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

    async fn refresh_file_runtime(&self) {
        let files = {
            let session = self.session.lock().await;
            session
                .as_ref()
                .and_then(|session| session.open_file_runtime().ok())
        };
        *self.files.lock().await = files;
    }

    async fn poll_files(&self) {
        let (changed, announcements) = {
            let mut files = self.files.lock().await;
            let Some(files) = files.as_mut() else {
                return;
            };
            let root_before = files.root_status();
            let changed_files = files.poll_root_changes().unwrap_or_default() > 0;
            let root_changed = root_before != files.root_status();
            (
                changed_files || root_changed,
                files.take_pending_announcements(),
            )
        };
        if !announcements.is_empty() {
            self.announce_file_changes(announcements).await;
        }
        if changed {
            self.emit_view().await;
        }
    }

    async fn announce_file_changes(&self, operation_ids: Vec<String>) {
        let recovery_service = self
            .files
            .lock()
            .await
            .as_ref()
            .and_then(|files| files.recovery_service().ok());
        let transport = self.transport.lock().await;
        let mut session = self.session.lock().await;
        if let Some(session) = session.as_mut() {
            if session.announce_file_history(operation_ids).is_err() {
                *self.issue.lock().await = Some(WorkspaceIssue::Storage);
                return;
            }
            if let Some(transport) = transport.as_ref() {
                if let Some(service) = recovery_service {
                    transport.configure_file_recovery(service).await;
                }
                if let Err(error) = transport.flush_session(session).await {
                    *self.issue.lock().await = Some(network_delivery_issue(error));
                }
            }
        }
    }

    async fn restart_transport(&self) {
        let recovery_service = self
            .files
            .lock()
            .await
            .as_ref()
            .and_then(|files| files.recovery_service().ok());
        let mut transport = self.transport.lock().await;
        if let Some(mut active) = transport.take() {
            let _ = active.shutdown().await;
        }
        let mut session = self.session.lock().await;
        let Some(session) = session.as_mut() else {
            return;
        };
        match IrohTransport::start_for_session(session).await {
            Ok(active) => {
                if let Some(service) = recovery_service {
                    active.configure_file_recovery(service).await;
                }
                if let Err(error) = active.flush_session(session).await {
                    *self.issue.lock().await = Some(network_delivery_issue(error));
                } else {
                    *self.issue.lock().await = None;
                }
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
                Ok(Ok(view_changed)) => {
                    let mut changed = view_changed;
                    if let Err(error) = transport.flush_session(session).await {
                        *self.issue.lock().await = Some(network_delivery_issue(error));
                        changed = true;
                    }
                    match transport.recover_file_history(session).await {
                        Ok(recovered) => changed |= recovered,
                        Err(error) => {
                            *self.issue.lock().await = Some(network_delivery_issue(error));
                            changed = true;
                        }
                    }
                    changed
                }
                Err(_) => false,
                Ok(Err(error)) => {
                    *self.issue.lock().await = Some(network_delivery_issue(error));
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
            self.refresh_file_runtime().await;
            self.refresh_file_recovery_service().await;
            self.emit_view().await;
        } else if !has_transport {
            time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn refresh_file_recovery_service(&self) {
        let recovery_service = self
            .files
            .lock()
            .await
            .as_ref()
            .and_then(|files| files.recovery_service().ok());
        if let (Some(transport), Some(service)) =
            (self.transport.lock().await.as_ref(), recovery_service)
        {
            transport.configure_file_recovery(service).await;
        }
    }

    async fn send_heartbeat_and_expire(&self) {
        let now = unix_seconds();
        let mut should_emit = {
            let mut session = self.session.lock().await;
            let Some(session) = session.as_mut() else {
                return;
            };
            if !session.has_active_workspace() {
                return;
            }
            match session.expire_presence(now) {
                Ok(changed) => changed,
                Err(_) => {
                    *self.issue.lock().await = Some(WorkspaceIssue::Storage);
                    true
                }
            }
        };
        let transport = self.transport.lock().await;
        let mut session = self.session.lock().await;
        if let (Some(transport), Some(session)) = (transport.as_ref(), session.as_mut()) {
            if let Err(error) = transport.send_session_heartbeat(session).await {
                *self.issue.lock().await = Some(network_delivery_issue(error));
                should_emit = true;
            }
        }
        drop(session);
        drop(transport);
        if should_emit {
            self.emit_view().await;
        }
    }
}

#[tauri::command]
pub async fn workspace_view(
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn choose_workspace_root(
    app: AppHandle,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    let Some(root) = choose_confirmed_root(&app)? else {
        return Ok(state.inner.view().await);
    };
    let root = root
        .into_path()
        .map_err(|_| "Resonance could not use that folder.".to_owned())?;
    {
        let mut files = state.inner.files.lock().await;
        files
            .as_mut()
            .ok_or_else(file_runtime_unavailable)?
            .bind_root(root, RootSelection::ConfirmedNotGitManaged)
            .map_err(|_| "Choose a new or empty folder outside Git management.".to_owned())?;
    }
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn replace_workspace_root(
    app: AppHandle,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    let Some(root) = choose_confirmed_root(&app)? else {
        return Ok(state.inner.view().await);
    };
    let root = root
        .into_path()
        .map_err(|_| "Resonance could not use that folder.".to_owned())?;
    {
        let mut files = state.inner.files.lock().await;
        files
            .as_mut()
            .ok_or_else(file_runtime_unavailable)?
            .replace_root(root, RootSelection::ConfirmedNotGitManaged)
            .map_err(|_| "Choose a new or empty folder outside Git management.".to_owned())?;
    }
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn repair_workspace_root(
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    {
        let mut files = state.inner.files.lock().await;
        files
            .as_mut()
            .ok_or_else(file_runtime_unavailable)?
            .repair_root()
            .map_err(|_| "The bound folder is still unavailable or not writable.".to_owned())?;
    }
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn unbind_workspace_root(
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    {
        let mut files = state.inner.files.lock().await;
        files
            .as_mut()
            .ok_or_else(file_runtime_unavailable)?
            .unbind_root()
            .map_err(|_| "Resonance could not clear the private root binding.".to_owned())?;
    }
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn open_markdown_file(
    request: OpenMarkdownRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<MarkdownRevisionView, String> {
    let files = state.inner.files.lock().await;
    files
        .as_ref()
        .ok_or_else(file_runtime_unavailable)?
        .open_markdown_file(&request.node_id, &request.revision_id)
        .map(markdown_revision_view)
        .map_err(|_| "That Markdown revision is unavailable.".to_owned())
}

#[tauri::command]
pub async fn create_markdown_file(
    request: CreateMarkdownRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<MarkdownRevisionView, String> {
    let (revision, announcements) = {
        let mut files = state.inner.files.lock().await;
        let files = files.as_mut().ok_or_else(file_runtime_unavailable)?;
        let revision = files
            .create_markdown_file(&request.parent_node_id, &request.name, &request.markdown)
            .map_err(|_| "Resonance could not create that Markdown file.".to_owned())?;
        (revision, files.take_pending_announcements())
    };
    state.inner.announce_file_changes(announcements).await;
    state.inner.emit_view().await;
    Ok(markdown_revision_view(revision))
}

#[tauri::command]
pub async fn replace_markdown_file(
    request: ReplaceMarkdownRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<MarkdownRevisionView, String> {
    let (revision, announcements) = {
        let mut files = state.inner.files.lock().await;
        let files = files.as_mut().ok_or_else(file_runtime_unavailable)?;
        let revision = files
            .replace_markdown_file(
                &request.node_id,
                &request.base_revision_id,
                &request.markdown,
            )
            .map_err(|_| {
                "The file changed before this edit could be saved. Reopen it and retry.".to_owned()
            })?;
        (revision, files.take_pending_announcements())
    };
    state.inner.announce_file_changes(announcements).await;
    state.inner.emit_view().await;
    Ok(markdown_revision_view(revision))
}

#[tauri::command]
pub async fn resolve_workspace_conflict(
    request: ResolveConflictRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    let announcements = {
        let mut files = state.inner.files.lock().await;
        let files = files.as_mut().ok_or_else(file_runtime_unavailable)?;
        files
            .resolve_conflict(&request.record_id, request.chosen_revision_id)
            .map_err(|_| "That conflict choice is no longer available.".to_owned())?;
        files.take_pending_announcements()
    };
    state.inner.announce_file_changes(announcements).await;
    state.inner.emit_view().await;
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
            .create_workspace_with_creator(
                request.display_name,
                request.creator_display_name,
                request.relay_override,
            )
            .map_err(|_| "Resonance could not create this workspace.".to_owned())?;
    }
    state.inner.refresh_file_runtime().await;
    state.inner.restart_transport().await;
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
    Invite::decode(&request.invite)
        .map_err(|_| "That invite is invalid or has been altered.".to_owned())?;
    {
        let mut session = state.inner.session.lock().await;
        let session = session.as_mut().ok_or_else(|| {
            "The installation identity or workspace storage is unavailable.".to_owned()
        })?;
        session
            .join_workspace(&request.invite, request.display_name)
            .map_err(|_| "Resonance could not join this workspace.".to_owned())?;
    }
    state.inner.refresh_file_runtime().await;
    state.inner.restart_transport().await;
    state.inner.emit_view().await;
    Ok(state.inner.view().await)
}

#[tauri::command]
pub async fn retry_workspace_join(
    request: RetryJoinRequest,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceShellView, String> {
    let retry_sent = {
        let mut session = state.inner.session.lock().await;
        let session = session.as_mut().ok_or_else(|| {
            "The installation identity or workspace storage is unavailable.".to_owned()
        })?;
        session
            .retry_join(request.display_name)
            .map_err(|_| "Resonance could not retry this workspace join.".to_owned())?
    };
    if retry_sent {
        state.inner.restart_transport().await;
        state.inner.emit_view().await;
    }
    state.inner.refresh_file_runtime().await;
    Ok(state.inner.view().await)
}

fn choose_confirmed_root(app: &AppHandle) -> Result<Option<tauri_plugin_dialog::FilePath>, String> {
    let Some(root) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let confirmed = app
        .dialog()
        .message("Use this folder only if it is new or empty and is not managed by Git.")
        .title("Confirm workspace folder")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Use folder".to_owned(),
            "Cancel".to_owned(),
        ))
        .blocking_show();
    if !confirmed {
        return Ok(None);
    }
    Ok(Some(root))
}

fn workspace_files_view(files: &WorkspaceFileRuntime) -> WorkspaceFilesView {
    WorkspaceFilesView {
        root: RootView {
            state: match files.root_status() {
                RootBindingStatus::Unbound => "unbound",
                RootBindingStatus::Bound(RootHealth::Healthy) => "healthy",
                RootBindingStatus::Bound(RootHealth::Unavailable) => "unavailable",
                RootBindingStatus::Bound(RootHealth::Unwritable) => "unwritable",
                RootBindingStatus::Bound(RootHealth::Unhealthy) => "unhealthy",
            }
            .to_owned(),
        },
        entries: files
            .tree_entries()
            .into_iter()
            .map(file_entry_view)
            .collect(),
        conflicts: files.conflicts().into_iter().map(conflict_view).collect(),
    }
}

fn file_entry_view(entry: FileTreeEntry) -> FileEntryView {
    FileEntryView {
        node_id: entry.node_id,
        parent_node_id: entry.parent_node_id,
        name: entry.name,
        kind: match entry.kind {
            FileEntryKind::Directory => "directory",
            FileEntryKind::Markdown => "markdown",
            FileEntryKind::Binary => "binary",
        }
        .to_owned(),
        current_revision_id: entry.current_revision_id,
        editable: entry.editable,
    }
}

fn conflict_view(conflict: FileConflictView) -> ConflictView {
    ConflictView {
        record_id: conflict.record_id,
        node_id: conflict.node_id,
        kind: match conflict.kind {
            ConflictKind::MarkdownOverlap => "markdown-overlap",
            ConflictKind::BinaryCollision => "binary-collision",
            ConflictKind::DeleteEdit => "delete-edit",
            ConflictKind::ConcurrentCreate => "concurrent-create",
            ConflictKind::CompetingMove => "competing-move",
        }
        .to_owned(),
        competing_revision_ids: conflict.competing_revision_ids,
    }
}

fn markdown_revision_view(revision: MarkdownFileView) -> MarkdownRevisionView {
    MarkdownRevisionView {
        node_id: revision.node_id,
        revision_id: revision.revision_id,
        markdown: revision.markdown,
    }
}

fn file_runtime_unavailable() -> String {
    "Workspace files are unavailable until membership is ready.".to_owned()
}

fn workspace_summary_view(workspace: &WorkspaceSummary) -> WorkspaceView {
    WorkspaceView {
        id: workspace.id.as_str().to_owned(),
        display_name: workspace.display_name.clone(),
        lifecycle: match workspace.lifecycle {
            WorkspaceLifecycle::Initializing => "initializing".to_owned(),
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

fn peer_view(peer: &KnownPeer, members: &[Member]) -> PeerView {
    let display_name = members
        .iter()
        .find(|member| member.public_identity == peer.public_identity)
        .map(|member| member.display_name.clone())
        .unwrap_or_else(|| peer.public_identity[..12.min(peer.public_identity.len())].to_owned());
    PeerView {
        public_identity: peer.public_identity.clone(),
        display_name,
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
            | IrohTransportError::BroadcastClosed
            | IrohTransportError::Receive
            | IrohTransportError::Shutdown
            | IrohTransportError::FileStream,
        )
        | IrohSessionAdapterError::Session(_)
        | IrohSessionAdapterError::FileRecovery(_) => {
            "Peer networking could not start for this workspace."
        }
    };
    WorkspaceIssue::Network(message.to_owned())
}

fn network_delivery_issue(error: IrohSessionAdapterError) -> WorkspaceIssue {
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
            return WorkspaceIssue::Network(format!(
                "Peer networking rejected this message: {reason}."
            ));
        }
        IrohSessionAdapterError::Session(WorkspaceSessionError::Protocol(_)) => {
            "Peer networking received an invalid workspace message."
        }
        IrohSessionAdapterError::Session(_) => "Peer networking could not apply workspace state.",
        _ => "Peer networking is offline. Local workspace data is still available.",
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
        issue_message, network_delivery_issue, network_start_issue, IrohSessionAdapterError,
        IrohTransportError, MemberView, PeerView, ReplaceMarkdownRequest, WorkspaceSessionError,
        WorkspaceShellView, WorkspaceView,
    };

    #[test]
    fn maps_transport_start_errors_to_safe_actionable_messages() {
        let message = issue_message(network_start_issue(IrohSessionAdapterError::Transport(
            IrohTransportError::Bootstrap,
        )));
        assert_eq!(message, "The invite's peer address is invalid.");
        let closed = issue_message(network_delivery_issue(IrohSessionAdapterError::Transport(
            IrohTransportError::BroadcastClosed,
        )));
        assert_eq!(
            closed,
            "Peer networking closed the workspace channel before it could send a message."
        );
        let rejected = issue_message(network_delivery_issue(IrohSessionAdapterError::Session(
            WorkspaceSessionError::InvalidInviteAdmission("test admission rejection"),
        )));
        assert_eq!(
            rejected,
            "Peer networking rejected this message: test admission rejection."
        );
        for forbidden in ["secret", "token", "bootstrap", "path", "iroh"] {
            assert!(!message.to_ascii_lowercase().contains(forbidden));
        }
    }

    #[test]
    fn markdown_replacement_request_rejects_private_or_unknown_fields() {
        let request = serde_json::json!({
            "nodeId": "node-id",
            "baseRevisionId": "revision-id",
            "markdown": "# Safe",
            "path": "/private/root/file.md"
        });

        assert!(serde_json::from_value::<ReplaceMarkdownRequest>(request).is_err());
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
                display_name: "Ada".to_owned(),
                online: true,
                connection: "direct".to_owned(),
            }],
            files: Some(super::WorkspaceFilesView {
                root: super::RootView {
                    state: "healthy".to_owned(),
                },
                entries: vec![super::FileEntryView {
                    node_id: "node-id".to_owned(),
                    parent_node_id: None,
                    name: "plans".to_owned(),
                    kind: "directory".to_owned(),
                    current_revision_id: None,
                    editable: true,
                }],
                conflicts: Vec::new(),
            }),
        };

        let payload = serde_json::to_string(&view).expect("view serializes");
        for forbidden in ["secret", "token", "private", "bootstrap", "path", "iroh"] {
            assert!(!payload.to_ascii_lowercase().contains(forbidden));
        }
    }
}
