use std::collections::{BTreeMap, BTreeSet};

use resonance_runtime::{
    local_root_binding::{RootHealth, RootSelection},
    workspace_file_runtime::{
        FileConflictChoiceKind, FileConflictChoiceView, FileConflictView, FileEntryKind,
        FilePreview, FileTreeEntry, MarkdownFileView, RootBindingStatus, WorkspaceFileRuntime,
        WorkspaceFileRuntimeError, MAX_IMAGE_PREVIEW_BYTES, MAX_MARKDOWN_BYTES,
    },
    workspace_files::projection::ConflictKind,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use super::workspace::{ManagedWorkspace, ManagedWorkspaceState};

const MAX_IDENTIFIER_LENGTH: usize = 128;
const MAX_NAME_LENGTH: usize = 255;
const MAX_TARGET_LOCATION_LENGTH: usize = 4096;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const MAX_ENTRIES: usize = 10_000;
const MAX_CONFLICTS: usize = 1_000;
const MAX_CONFLICT_ITEMS: usize = 100;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkspaceFilesRequest {
    Snapshot,
    SelectRoot,
    ReplaceRoot,
    RepairRoot,
    UnbindRoot,
    OpenMarkdown {
        node_id: String,
        revision_id: String,
    },
    OpenPreview {
        node_id: String,
        revision_id: String,
    },
    CreateMarkdown {
        parent_node_id: String,
        name: String,
        markdown: String,
    },
    ReplaceMarkdown {
        node_id: String,
        base_revision_id: String,
        markdown: String,
    },
    ResolveConflict {
        record_id: String,
        chosen_candidate_id: Option<String>,
    },
}

impl WorkspaceFilesRequest {
    fn parse(value: Value) -> Result<Self, WorkspaceFilesError> {
        let object = value
            .as_object()
            .ok_or_else(|| WorkspaceFilesError::new(WorkspaceFilesErrorCode::InvalidRequest))?;
        let operation = object
            .get("operation")
            .and_then(Value::as_str)
            .ok_or_else(|| WorkspaceFilesError::new(WorkspaceFilesErrorCode::InvalidRequest))?;
        let allowed: &[&str] = match operation {
            "snapshot" | "select-root" | "replace-root" | "repair-root" | "unbind-root" => {
                &["operation"]
            }
            "open-markdown" | "open-preview" => &["operation", "nodeId", "revisionId"],
            "create-markdown" => &["operation", "parentNodeId", "name", "markdown"],
            "replace-markdown" => &["operation", "nodeId", "baseRevisionId", "markdown"],
            "resolve-conflict" => &["operation", "recordId", "chosenCandidateId"],
            _ => {
                return Err(WorkspaceFilesError::new(
                    WorkspaceFilesErrorCode::InvalidRequest,
                ))
            }
        };
        if object.keys().any(|key| !allowed.contains(&key.as_str())) {
            return Err(WorkspaceFilesError::new(
                WorkspaceFilesErrorCode::InvalidRequest,
            ));
        }
        let request: Self = serde_json::from_value(value)
            .map_err(|_| WorkspaceFilesError::new(WorkspaceFilesErrorCode::InvalidRequest))?;
        request
            .validate()
            .then_some(request)
            .ok_or_else(|| WorkspaceFilesError::new(WorkspaceFilesErrorCode::InvalidRequest))
    }

    fn validate(&self) -> bool {
        match self {
            Self::Snapshot
            | Self::SelectRoot
            | Self::ReplaceRoot
            | Self::RepairRoot
            | Self::UnbindRoot => true,
            Self::OpenMarkdown {
                node_id,
                revision_id,
            }
            | Self::OpenPreview {
                node_id,
                revision_id,
            } => valid_identifier(node_id) && valid_identifier(revision_id),
            Self::CreateMarkdown {
                parent_node_id,
                name,
                markdown,
            } => {
                valid_identifier(parent_node_id)
                    && !name.is_empty()
                    && name.chars().count() <= MAX_NAME_LENGTH
                    && markdown.len() <= MAX_MARKDOWN_BYTES
            }
            Self::ReplaceMarkdown {
                node_id,
                base_revision_id,
                markdown,
            } => {
                valid_identifier(node_id)
                    && valid_identifier(base_revision_id)
                    && markdown.len() <= MAX_MARKDOWN_BYTES
            }
            Self::ResolveConflict {
                record_id,
                chosen_candidate_id,
            } => {
                valid_identifier(record_id)
                    && chosen_candidate_id
                        .as_ref()
                        .is_none_or(|candidate| valid_identifier(candidate))
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "operation",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum WorkspaceFilesResponse {
    Snapshot { snapshot: WorkspaceFilesSnapshot },
    SelectRoot { snapshot: WorkspaceFilesSnapshot },
    ReplaceRoot { snapshot: WorkspaceFilesSnapshot },
    RepairRoot { snapshot: WorkspaceFilesSnapshot },
    UnbindRoot { snapshot: WorkspaceFilesSnapshot },
    OpenMarkdown { revision: MarkdownRevisionView },
    OpenPreview { preview: FilePreviewView },
    CreateMarkdown { revision: MarkdownRevisionView },
    ReplaceMarkdown { revision: MarkdownRevisionView },
    ResolveConflict { snapshot: WorkspaceFilesSnapshot },
}

impl WorkspaceFilesResponse {
    fn validate(&self) -> bool {
        match self {
            Self::Snapshot { snapshot }
            | Self::SelectRoot { snapshot }
            | Self::ReplaceRoot { snapshot }
            | Self::RepairRoot { snapshot }
            | Self::UnbindRoot { snapshot }
            | Self::ResolveConflict { snapshot } => snapshot.validate(),
            Self::OpenMarkdown { revision }
            | Self::CreateMarkdown { revision }
            | Self::ReplaceMarkdown { revision } => revision.validate(),
            Self::OpenPreview { preview } => preview.validate(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceFilesSnapshot {
    pub root: RootView,
    pub entries: Vec<FileEntryView>,
    pub conflicts: Vec<ConflictView>,
}

impl WorkspaceFilesSnapshot {
    fn validate(&self) -> bool {
        if self.entries.len() > MAX_ENTRIES
            || self.conflicts.len() > MAX_CONFLICTS
            || !self.entries.iter().all(FileEntryView::validate)
            || !self.conflicts.iter().all(ConflictView::validate)
        {
            return false;
        }

        let node_ids = self
            .entries
            .iter()
            .map(|entry| entry.node_id.as_str())
            .collect::<BTreeSet<_>>();
        if node_ids.len() != self.entries.len()
            || (!self.entries.is_empty()
                && self
                    .entries
                    .iter()
                    .filter(|entry| entry.parent_node_id.is_none())
                    .count()
                    != 1)
        {
            return false;
        }

        let parents = self
            .entries
            .iter()
            .map(|entry| (entry.node_id.as_str(), entry.parent_node_id.as_deref()))
            .collect::<BTreeMap<_, _>>();
        for entry in &self.entries {
            if entry
                .parent_node_id
                .as_deref()
                .is_some_and(|parent| !node_ids.contains(parent))
            {
                return false;
            }
            let mut visited = BTreeSet::new();
            let mut current = Some(entry.node_id.as_str());
            while let Some(node_id) = current {
                if !visited.insert(node_id) {
                    return false;
                }
                current = parents.get(node_id).copied().flatten();
            }
        }

        let record_ids = self
            .conflicts
            .iter()
            .map(|conflict| conflict.record_id.as_str())
            .collect::<BTreeSet<_>>();
        record_ids.len() == self.conflicts.len()
            && self
                .conflicts
                .iter()
                .all(|conflict| conflict.validate_references(&node_ids))
    }
}

pub type WorkspaceFilesView = WorkspaceFilesSnapshot;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootView {
    pub state: RootState,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RootState {
    Unbound,
    Healthy,
    Unavailable,
    Unwritable,
    Unhealthy,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEntryView {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub name: String,
    pub kind: FileEntryViewKind,
    pub current_revision_id: Option<String>,
    pub editable: bool,
}

impl FileEntryView {
    fn validate(&self) -> bool {
        valid_identifier(&self.node_id)
            && self
                .parent_node_id
                .as_ref()
                .is_none_or(|value| valid_identifier(value))
            && !self.name.is_empty()
            && self.name.chars().count() <= MAX_NAME_LENGTH
            && self
                .current_revision_id
                .as_ref()
                .is_none_or(|value| valid_identifier(value))
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileEntryViewKind {
    Directory,
    Markdown,
    Binary,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConflictView {
    pub record_id: String,
    pub node_id: String,
    pub kind: ConflictViewKind,
    pub competing_revision_ids: Vec<String>,
    pub resolution_candidate_ids: Vec<String>,
    pub reviewable_revision_ids: Vec<String>,
    pub deletion_operation_id: Option<String>,
    pub tree_choices: Vec<ConflictChoiceView>,
}

impl ConflictView {
    fn validate(&self) -> bool {
        valid_identifier(&self.record_id)
            && valid_identifier(&self.node_id)
            && valid_identifiers(&self.competing_revision_ids)
            && valid_identifiers(&self.resolution_candidate_ids)
            && valid_identifiers(&self.reviewable_revision_ids)
            && self
                .deletion_operation_id
                .as_ref()
                .is_none_or(|value| valid_identifier(value))
            && self.tree_choices.len() <= MAX_CONFLICT_ITEMS
            && self.tree_choices.iter().all(ConflictChoiceView::validate)
    }

    fn validate_references(&self, node_ids: &BTreeSet<&str>) -> bool {
        if self.kind != ConflictViewKind::ConcurrentCreate
            && !node_ids.contains(self.node_id.as_str())
        {
            return false;
        }
        let tree_candidate_ids = self
            .tree_choices
            .iter()
            .map(|choice| choice.candidate_id.as_str())
            .collect::<BTreeSet<_>>();
        if tree_candidate_ids.len() != self.tree_choices.len() {
            return false;
        }
        let mut known_candidates = self
            .competing_revision_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        known_candidates.extend(tree_candidate_ids);
        if let Some(deletion) = self.deletion_operation_id.as_deref() {
            known_candidates.insert(deletion);
        }
        let competing_revisions = self
            .competing_revision_ids
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        self.resolution_candidate_ids
            .iter()
            .all(|candidate| known_candidates.contains(candidate.as_str()))
            && self
                .reviewable_revision_ids
                .iter()
                .all(|revision| competing_revisions.contains(revision.as_str()))
            && self.tree_choices.iter().all(|choice| {
                choice.kind != ConflictChoiceViewKind::Move
                    || node_ids.contains(choice.node_id.as_str())
            })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictViewKind {
    MarkdownOverlap,
    BinaryCollision,
    DeleteEdit,
    ConcurrentCreate,
    CompetingMove,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConflictChoiceView {
    pub candidate_id: String,
    pub node_id: String,
    pub kind: ConflictChoiceViewKind,
    pub selected: bool,
    pub name: String,
    pub target_location: Option<String>,
}

impl ConflictChoiceView {
    fn validate(&self) -> bool {
        valid_identifier(&self.candidate_id)
            && valid_identifier(&self.node_id)
            && !self.name.is_empty()
            && self.name.chars().count() <= MAX_NAME_LENGTH
            && self.target_location.as_ref().is_none_or(|value| {
                !value.is_empty() && value.chars().count() <= MAX_TARGET_LOCATION_LENGTH
            })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConflictChoiceViewKind {
    File,
    Directory,
    Move,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum FilePreviewView {
    Image {
        mime_type: ImageMimeType,
        bytes: Vec<u8>,
        byte_length: u64,
    },
    Unavailable {
        mime_type: String,
        bytes: Vec<u8>,
        byte_length: u64,
    },
}

impl FilePreviewView {
    fn validate(&self) -> bool {
        match self {
            Self::Image {
                bytes, byte_length, ..
            } => {
                bytes.len() <= MAX_IMAGE_PREVIEW_BYTES
                    && *byte_length <= MAX_IMAGE_PREVIEW_BYTES as u64
                    && *byte_length == bytes.len() as u64
            }
            Self::Unavailable {
                mime_type,
                bytes,
                byte_length,
            } => {
                !mime_type.is_empty()
                    && mime_type.chars().count() <= MAX_NAME_LENGTH
                    && bytes.is_empty()
                    && *byte_length <= MAX_SAFE_INTEGER
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub enum ImageMimeType {
    #[serde(rename = "image/png")]
    Png,
    #[serde(rename = "image/jpeg")]
    Jpeg,
    #[serde(rename = "image/gif")]
    Gif,
    #[serde(rename = "image/webp")]
    Webp,
    #[serde(rename = "image/bmp")]
    Bmp,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownRevisionView {
    pub node_id: String,
    pub revision_id: String,
    pub markdown: String,
}

impl MarkdownRevisionView {
    fn validate(&self) -> bool {
        valid_identifier(&self.node_id)
            && valid_identifier(&self.revision_id)
            && self.markdown.len() <= MAX_MARKDOWN_BYTES
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkspaceFilesError {
    pub code: WorkspaceFilesErrorCode,
    pub message: String,
}

impl WorkspaceFilesError {
    fn new(code: WorkspaceFilesErrorCode) -> Self {
        Self {
            code,
            message: code.message().to_owned(),
        }
    }

    #[cfg(test)]
    fn validate(&self) -> bool {
        self.message == self.code.message()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceFilesErrorCode {
    UnavailableCapability,
    InvalidRequest,
    MissingRevision,
    StaleRevision,
    InvalidMarkdownName,
    SizeLimit,
    UnusableRoot,
    ChangedConflictChoice,
    Internal,
}

impl WorkspaceFilesErrorCode {
    fn message(self) -> &'static str {
        match self {
            Self::UnavailableCapability => "Workspace files are unavailable.",
            Self::InvalidRequest => "The workspace-files request is invalid.",
            Self::MissingRevision => "That file revision is unavailable.",
            Self::StaleRevision => "The file changed before this edit could be saved.",
            Self::InvalidMarkdownName => "Markdown file names must end in .md.",
            Self::SizeLimit => "The workspace-files size limit was exceeded.",
            Self::UnusableRoot => "Choose a usable workspace folder.",
            Self::ChangedConflictChoice => "That conflict choice is no longer available.",
            Self::Internal => "Workspace files could not complete the request.",
        }
    }
}

#[tauri::command]
pub async fn workspace_files_v1(
    request: Value,
    app: AppHandle,
    state: State<'_, ManagedWorkspaceState>,
) -> Result<WorkspaceFilesResponse, WorkspaceFilesError> {
    let request = WorkspaceFilesRequest::parse(request)?;
    let response = dispatch(request, &app, &state).await?;
    response
        .validate()
        .then_some(response)
        .ok_or_else(|| WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal))
}

async fn dispatch(
    request: WorkspaceFilesRequest,
    app: &AppHandle,
    state: &ManagedWorkspaceState,
) -> Result<WorkspaceFilesResponse, WorkspaceFilesError> {
    match request {
        WorkspaceFilesRequest::Snapshot => Ok(WorkspaceFilesResponse::Snapshot {
            snapshot: snapshot(&state.inner).await?,
        }),
        WorkspaceFilesRequest::SelectRoot | WorkspaceFilesRequest::ReplaceRoot => {
            let replace = matches!(request, WorkspaceFilesRequest::ReplaceRoot);
            if let Some(root) = choose_confirmed_root(app)? {
                state
                    .inner
                    .with_files(|files| {
                        let files = files.ok_or_else(unavailable)?;
                        if replace {
                            files.replace_root(root, RootSelection::ConfirmedNotGitManaged)
                        } else {
                            files.bind_root(root, RootSelection::ConfirmedNotGitManaged)
                        }
                        .map_err(root_error)
                    })
                    .await?;
                state.inner.emit_files_changed();
            }
            let response = if replace {
                WorkspaceFilesResponse::ReplaceRoot {
                    snapshot: snapshot(&state.inner).await?,
                }
            } else {
                WorkspaceFilesResponse::SelectRoot {
                    snapshot: snapshot(&state.inner).await?,
                }
            };
            Ok(response)
        }
        WorkspaceFilesRequest::RepairRoot | WorkspaceFilesRequest::UnbindRoot => {
            let unbind = matches!(request, WorkspaceFilesRequest::UnbindRoot);
            state
                .inner
                .with_files(|files| {
                    let files = files.ok_or_else(unavailable)?;
                    if unbind {
                        files.unbind_root()
                    } else {
                        files.repair_root()
                    }
                    .map_err(root_error)
                })
                .await?;
            state.inner.emit_files_changed();
            let snapshot = snapshot(&state.inner).await?;
            Ok(if unbind {
                WorkspaceFilesResponse::UnbindRoot { snapshot }
            } else {
                WorkspaceFilesResponse::RepairRoot { snapshot }
            })
        }
        WorkspaceFilesRequest::OpenMarkdown {
            node_id,
            revision_id,
        } => {
            let revision = state
                .inner
                .with_files(|files| {
                    files.ok_or_else(unavailable).and_then(|files| {
                        files
                            .open_markdown_file(&node_id, &revision_id)
                            .map(markdown_revision_view)
                            .map_err(revision_error)
                    })
                })
                .await?;
            Ok(WorkspaceFilesResponse::OpenMarkdown { revision })
        }
        WorkspaceFilesRequest::OpenPreview {
            node_id,
            revision_id,
        } => {
            let preview = state
                .inner
                .with_files(|files| {
                    files.ok_or_else(unavailable).and_then(|files| {
                        files
                            .open_file_preview(&node_id, &revision_id)
                            .map(file_preview_view)
                            .map_err(revision_error)
                    })
                })
                .await?;
            Ok(WorkspaceFilesResponse::OpenPreview { preview })
        }
        WorkspaceFilesRequest::CreateMarkdown {
            parent_node_id,
            name,
            markdown,
        } => {
            let (revision, announcements) = state
                .inner
                .with_files(|files| {
                    let files = files.ok_or_else(unavailable)?;
                    let revision = files
                        .create_markdown_file(&parent_node_id, &name, &markdown)
                        .map_err(mutation_error)?;
                    Ok::<_, WorkspaceFilesError>((revision, files.take_pending_announcements()))
                })
                .await?;
            state.inner.announce_file_changes(announcements).await;
            state.inner.emit_files_changed();
            Ok(WorkspaceFilesResponse::CreateMarkdown {
                revision: markdown_revision_view(revision),
            })
        }
        WorkspaceFilesRequest::ReplaceMarkdown {
            node_id,
            base_revision_id,
            markdown,
        } => {
            let (revision, announcements) = state
                .inner
                .with_files(|files| {
                    let files = files.ok_or_else(unavailable)?;
                    let revision = files
                        .replace_markdown_file(&node_id, &base_revision_id, &markdown)
                        .map_err(mutation_error)?;
                    Ok::<_, WorkspaceFilesError>((revision, files.take_pending_announcements()))
                })
                .await?;
            state.inner.announce_file_changes(announcements).await;
            state.inner.emit_files_changed();
            Ok(WorkspaceFilesResponse::ReplaceMarkdown {
                revision: markdown_revision_view(revision),
            })
        }
        WorkspaceFilesRequest::ResolveConflict {
            record_id,
            chosen_candidate_id,
        } => {
            let announcements = state
                .inner
                .with_files(|files| {
                    let files = files.ok_or_else(unavailable)?;
                    files
                        .resolve_conflict(&record_id, chosen_candidate_id)
                        .map_err(|_| {
                            WorkspaceFilesError::new(WorkspaceFilesErrorCode::ChangedConflictChoice)
                        })?;
                    Ok::<_, WorkspaceFilesError>(files.take_pending_announcements())
                })
                .await?;
            state.inner.announce_file_changes(announcements).await;
            state.inner.emit_files_changed();
            Ok(WorkspaceFilesResponse::ResolveConflict {
                snapshot: snapshot(&state.inner).await?,
            })
        }
    }
}

async fn snapshot(
    workspace: &ManagedWorkspace,
) -> Result<WorkspaceFilesSnapshot, WorkspaceFilesError> {
    workspace
        .with_files(|files| {
            files
                .as_deref()
                .map(workspace_files_view)
                .ok_or_else(unavailable)
        })
        .await
}

fn choose_confirmed_root(
    app: &AppHandle,
) -> Result<Option<std::path::PathBuf>, WorkspaceFilesError> {
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
    root.into_path()
        .map(Some)
        .map_err(|_| WorkspaceFilesError::new(WorkspaceFilesErrorCode::UnusableRoot))
}

pub(super) fn workspace_files_view(files: &WorkspaceFileRuntime) -> WorkspaceFilesView {
    WorkspaceFilesSnapshot {
        root: RootView {
            state: match files.root_status() {
                RootBindingStatus::Unbound => RootState::Unbound,
                RootBindingStatus::Bound(RootHealth::Healthy) => RootState::Healthy,
                RootBindingStatus::Bound(RootHealth::Unavailable) => RootState::Unavailable,
                RootBindingStatus::Bound(RootHealth::Unwritable) => RootState::Unwritable,
                RootBindingStatus::Bound(RootHealth::Unhealthy) => RootState::Unhealthy,
            },
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
            FileEntryKind::Directory => FileEntryViewKind::Directory,
            FileEntryKind::Markdown => FileEntryViewKind::Markdown,
            FileEntryKind::Binary => FileEntryViewKind::Binary,
        },
        current_revision_id: entry.current_revision_id,
        editable: entry.editable,
    }
}

fn conflict_view(conflict: FileConflictView) -> ConflictView {
    ConflictView {
        record_id: conflict.record_id,
        node_id: conflict.node_id,
        kind: match conflict.kind {
            ConflictKind::MarkdownOverlap => ConflictViewKind::MarkdownOverlap,
            ConflictKind::BinaryCollision => ConflictViewKind::BinaryCollision,
            ConflictKind::DeleteEdit => ConflictViewKind::DeleteEdit,
            ConflictKind::ConcurrentCreate => ConflictViewKind::ConcurrentCreate,
            ConflictKind::CompetingMove => ConflictViewKind::CompetingMove,
        },
        competing_revision_ids: conflict.competing_revision_ids,
        resolution_candidate_ids: conflict.resolution_candidate_ids,
        reviewable_revision_ids: conflict.reviewable_revision_ids,
        deletion_operation_id: conflict.deletion_operation_id,
        tree_choices: conflict
            .tree_choices
            .into_iter()
            .map(conflict_choice_view)
            .collect(),
    }
}

fn conflict_choice_view(choice: FileConflictChoiceView) -> ConflictChoiceView {
    ConflictChoiceView {
        candidate_id: choice.candidate_id,
        node_id: choice.node_id,
        kind: match choice.kind {
            FileConflictChoiceKind::File => ConflictChoiceViewKind::File,
            FileConflictChoiceKind::Directory => ConflictChoiceViewKind::Directory,
            FileConflictChoiceKind::Move => ConflictChoiceViewKind::Move,
        },
        selected: choice.selected,
        name: choice.name,
        target_location: choice.target_path,
    }
}

fn file_preview_view(preview: FilePreview) -> FilePreviewView {
    match preview {
        FilePreview::Image { mime_type, bytes } => FilePreviewView::Image {
            mime_type: image_mime_type(&mime_type),
            byte_length: bytes.len() as u64,
            bytes,
        },
        FilePreview::Unavailable {
            mime_type,
            byte_length,
        } => FilePreviewView::Unavailable {
            mime_type,
            bytes: Vec::new(),
            byte_length,
        },
    }
}

fn image_mime_type(mime_type: &str) -> ImageMimeType {
    match mime_type {
        "image/jpeg" => ImageMimeType::Jpeg,
        "image/gif" => ImageMimeType::Gif,
        "image/webp" => ImageMimeType::Webp,
        "image/bmp" => ImageMimeType::Bmp,
        _ => ImageMimeType::Png,
    }
}

fn markdown_revision_view(revision: MarkdownFileView) -> MarkdownRevisionView {
    MarkdownRevisionView {
        node_id: revision.node_id,
        revision_id: revision.revision_id,
        markdown: revision.markdown,
    }
}

fn unavailable() -> WorkspaceFilesError {
    WorkspaceFilesError::new(WorkspaceFilesErrorCode::UnavailableCapability)
}

fn revision_error(error: WorkspaceFileRuntimeError) -> WorkspaceFilesError {
    match error {
        WorkspaceFileRuntimeError::NotFound | WorkspaceFileRuntimeError::NotMarkdown => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::MissingRevision)
        }
        WorkspaceFileRuntimeError::TooLarge => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::SizeLimit)
        }
        _ => WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal),
    }
}

fn mutation_error(error: WorkspaceFileRuntimeError) -> WorkspaceFilesError {
    match error {
        WorkspaceFileRuntimeError::NotFound | WorkspaceFileRuntimeError::NotMarkdown => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::MissingRevision)
        }
        WorkspaceFileRuntimeError::StaleRevision => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::StaleRevision)
        }
        WorkspaceFileRuntimeError::InvalidName => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::InvalidMarkdownName)
        }
        WorkspaceFileRuntimeError::TooLarge => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::SizeLimit)
        }
        _ => WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal),
    }
}

fn root_error(_: WorkspaceFileRuntimeError) -> WorkspaceFilesError {
    WorkspaceFilesError::new(WorkspaceFilesErrorCode::UnusableRoot)
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= MAX_IDENTIFIER_LENGTH
}

fn valid_identifiers(values: &[String]) -> bool {
    values.len() <= MAX_CONFLICT_ITEMS
        && values.iter().all(|value| valid_identifier(value))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CORPUS: &str = include_str!(
        "../../../../../packages/contracts/fixtures/workspace-files-v1/valid/corpus.json"
    );
    const INVALID_CORPUS: &str = include_str!(
        "../../../../../packages/contracts/fixtures/workspace-files-v1/invalid/corpus.json"
    );

    #[derive(Deserialize)]
    struct Envelope {
        kind: String,
        value: Value,
    }

    #[derive(Deserialize)]
    struct InvalidFixture {
        name: String,
        value: Value,
    }

    fn valid_envelope(value: Value) -> bool {
        serde_json::from_value::<Envelope>(value).is_ok_and(|envelope| {
            match envelope.kind.as_str() {
                "request" => WorkspaceFilesRequest::parse(envelope.value).is_ok(),
                "response" => serde_json::from_value::<WorkspaceFilesResponse>(envelope.value)
                    .is_ok_and(|response| response.validate()),
                "error" => serde_json::from_value::<WorkspaceFilesError>(envelope.value)
                    .is_ok_and(|error| error.validate()),
                _ => false,
            }
        })
    }

    #[test]
    fn shares_the_complete_typescript_fixture_corpus() {
        let valid: Vec<Value> = serde_json::from_str(VALID_CORPUS).expect("valid corpus parses");
        assert!(valid.into_iter().all(valid_envelope));

        let invalid: Vec<InvalidFixture> =
            serde_json::from_str(INVALID_CORPUS).expect("invalid corpus parses");
        for fixture in invalid {
            assert!(
                !valid_envelope(fixture.value),
                "invalid fixture unexpectedly passed: {}",
                fixture.name
            );
        }
    }

    #[test]
    fn rejects_unknown_private_fields_and_oversized_utf8_markdown() {
        let private = serde_json::json!({ "operation": "snapshot", "path": "/private/root" });
        assert!(WorkspaceFilesRequest::parse(private).is_err());
        let oversized = WorkspaceFilesRequest::CreateMarkdown {
            parent_node_id: "plans".to_owned(),
            name: "large.md".to_owned(),
            markdown: "é".repeat(MAX_MARKDOWN_BYTES / 2 + 1),
        };
        assert!(!oversized.validate());
    }

    #[test]
    fn finite_errors_never_serialize_internal_details() {
        for code in [
            WorkspaceFilesErrorCode::UnavailableCapability,
            WorkspaceFilesErrorCode::InvalidRequest,
            WorkspaceFilesErrorCode::MissingRevision,
            WorkspaceFilesErrorCode::StaleRevision,
            WorkspaceFilesErrorCode::InvalidMarkdownName,
            WorkspaceFilesErrorCode::SizeLimit,
            WorkspaceFilesErrorCode::UnusableRoot,
            WorkspaceFilesErrorCode::ChangedConflictChoice,
            WorkspaceFilesErrorCode::Internal,
        ] {
            let serialized = serde_json::to_string(&WorkspaceFilesError::new(code))
                .expect("safe error serializes");
            for forbidden in ["/private", "sqlite", "token", "privateKey", "iroh"] {
                assert!(!serialized.contains(forbidden));
            }
        }
    }
}
