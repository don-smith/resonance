//! Owned composition for workspace file authority, private blobs, and local roots.

use std::path::Path;

use crate::{
    identity::InstallationIdentity,
    local_root_binding::{LocalRootBinding, RootBindingError, RootHealth, RootSelection},
    membership_log::MembershipProjection,
    workspace_catalog::WorkspaceCatalogError,
    workspace_file_transport::FileRecoveryService,
    workspace_files::{
        authority::{AuthorityError, WorkspaceFileAuthority},
        blobs::{BlobError, ContentHash},
        projection::{ConflictKind, ConflictRecord, FileTreeProjection, TreeNode},
        FileOperationBody, SignedFileOperation,
    },
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

pub const MAX_MARKDOWN_BYTES: usize = 1024 * 1024;
pub const MAX_IMAGE_PREVIEW_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootBindingStatus {
    Unbound,
    Bound(RootHealth),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileEntryKind {
    Directory,
    Markdown,
    Binary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileTreeEntry {
    pub node_id: String,
    pub parent_node_id: Option<String>,
    pub name: String,
    pub kind: FileEntryKind,
    pub current_revision_id: Option<String>,
    pub editable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownFileView {
    pub node_id: String,
    pub revision_id: String,
    pub markdown: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileConflictChoiceKind {
    File,
    Directory,
    Move,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileConflictChoiceView {
    pub candidate_id: String,
    pub node_id: String,
    pub kind: FileConflictChoiceKind,
    pub selected: bool,
    pub name: String,
    pub target_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FilePreview {
    Image { mime_type: String, bytes: Vec<u8> },
    Unavailable { mime_type: String, byte_length: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileConflictView {
    pub record_id: String,
    pub node_id: String,
    pub kind: ConflictKind,
    pub competing_revision_ids: Vec<String>,
    pub resolution_candidate_ids: Vec<String>,
    pub reviewable_revision_ids: Vec<String>,
    pub deletion_operation_id: Option<String>,
    pub tree_choices: Vec<FileConflictChoiceView>,
}

#[derive(Debug)]
pub enum WorkspaceFileRuntimeError {
    Catalog(WorkspaceCatalogError),
    Store(WorkspaceStoreError),
    Authority(AuthorityError),
    Blob(BlobError),
    Root(RootBindingError),
    NotFound,
    NotMarkdown,
    StaleRevision,
    InvalidName,
    TooLarge,
}

impl std::fmt::Display for WorkspaceFileRuntimeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Catalog(error) => write!(formatter, "workspace catalog failed: {error}"),
            Self::Store(error) => write!(formatter, "workspace file storage failed: {error}"),
            Self::Authority(error) => write!(formatter, "workspace file authority failed: {error}"),
            Self::Blob(error) => write!(formatter, "workspace blob storage failed: {error}"),
            Self::Root(error) => write!(formatter, "local root failed: {error}"),
            Self::NotFound => formatter.write_str("workspace file was not found"),
            Self::NotMarkdown => formatter.write_str("workspace file is not editable Markdown"),
            Self::StaleRevision => {
                formatter.write_str("workspace file revision is no longer current")
            }
            Self::InvalidName => formatter.write_str("Markdown file name must end in .md"),
            Self::TooLarge => formatter.write_str("Markdown file exceeds the editor byte limit"),
        }
    }
}

impl std::error::Error for WorkspaceFileRuntimeError {}

impl From<WorkspaceCatalogError> for WorkspaceFileRuntimeError {
    fn from(error: WorkspaceCatalogError) -> Self {
        Self::Catalog(error)
    }
}

impl From<WorkspaceStoreError> for WorkspaceFileRuntimeError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Store(error)
    }
}

impl From<AuthorityError> for WorkspaceFileRuntimeError {
    fn from(error: AuthorityError) -> Self {
        Self::Authority(error)
    }
}

impl From<BlobError> for WorkspaceFileRuntimeError {
    fn from(error: BlobError) -> Self {
        Self::Blob(error)
    }
}

impl From<RootBindingError> for WorkspaceFileRuntimeError {
    fn from(error: RootBindingError) -> Self {
        Self::Root(error)
    }
}

pub struct WorkspaceFileRuntime {
    workspace_id: String,
    identity: InstallationIdentity,
    membership: MembershipProjection,
    store: WorkspaceStore,
    authority: WorkspaceFileAuthority,
    root: Option<LocalRootBinding>,
    pending_announcements: Vec<String>,
}

impl WorkspaceFileRuntime {
    pub(crate) fn open(
        workspace_id: impl Into<String>,
        identity: InstallationIdentity,
        membership: MembershipProjection,
        store: WorkspaceStore,
    ) -> Result<Self, WorkspaceFileRuntimeError> {
        let workspace_id = workspace_id.into();
        let blobs = store.open_blob_store()?;
        let mut authority = WorkspaceFileAuthority::with_blob_store(&workspace_id, blobs);
        authority.replay(&store.file_operations()?, &membership)?;
        let root =
            LocalRootBinding::reopen(&store, &authority.projection(), authority.blob_store())?;
        Ok(Self {
            workspace_id,
            identity,
            membership,
            store,
            authority,
            root,
            pending_announcements: Vec::new(),
        })
    }

    #[must_use]
    pub fn root_status(&self) -> RootBindingStatus {
        self.root
            .as_ref()
            .map_or(RootBindingStatus::Unbound, |root| {
                RootBindingStatus::Bound(root.health())
            })
    }

    #[must_use]
    pub fn tree_entries(&self) -> Vec<FileTreeEntry> {
        let projection = self.authority.projection();
        let mut entries = Vec::new();
        for node in projection.root.values() {
            flatten_node(node, None, &projection, &mut entries);
        }
        entries
    }

    #[must_use]
    pub fn conflicts(&self) -> Vec<FileConflictView> {
        let projection = self.authority.projection();
        projection
            .conflicts
            .iter()
            .filter(|conflict| !conflict.resolved)
            .cloned()
            .map(|mut conflict| {
                let tree_choices = self.tree_conflict_choices(&projection, &conflict);
                let resolution_candidate_ids = match conflict.kind {
                    ConflictKind::ConcurrentCreate | ConflictKind::CompetingMove => {
                        conflict.competing_revision_ids.clone()
                    }
                    _ => conflict
                        .competing_revision_ids
                        .iter()
                        .filter(|candidate| projection.revisions.contains_key(*candidate))
                        .cloned()
                        .collect(),
                };
                let deletion_operation_id = (conflict.kind == ConflictKind::DeleteEdit)
                    .then(|| {
                        conflict
                            .competing_revision_ids
                            .iter()
                            .find(|candidate| !projection.revisions.contains_key(*candidate))
                            .cloned()
                    })
                    .flatten();
                conflict
                    .competing_revision_ids
                    .retain(|revision_id| projection.revisions.contains_key(revision_id));
                let reviewable_revision_ids = conflict
                    .competing_revision_ids
                    .iter()
                    .filter(|revision_id| {
                        projection
                            .revisions
                            .get(*revision_id)
                            .is_some_and(|revision| revision.mime_type == "text/markdown")
                    })
                    .cloned()
                    .collect();
                conflict_view(
                    conflict,
                    resolution_candidate_ids,
                    reviewable_revision_ids,
                    deletion_operation_id,
                    tree_choices,
                )
            })
            .collect()
    }

    pub fn open_file_preview(
        &self,
        node_id: &str,
        revision_id: &str,
    ) -> Result<FilePreview, WorkspaceFileRuntimeError> {
        let projection = self.authority.projection();
        let _entry = find_entry(&projection, node_id).ok_or(WorkspaceFileRuntimeError::NotFound)?;
        let revision = projection
            .revisions
            .get(revision_id)
            .filter(|revision| revision.node_id == node_id)
            .ok_or(WorkspaceFileRuntimeError::NotFound)?;
        if !is_previewable_image(&revision.mime_type)
            || revision.byte_length > MAX_IMAGE_PREVIEW_BYTES as u64
        {
            return Ok(FilePreview::Unavailable {
                mime_type: revision.mime_type.clone(),
                byte_length: revision.byte_length,
            });
        }
        let bytes = self
            .authority
            .blob_store()
            .open(&ContentHash(revision.content_hash.clone()))?;
        if bytes.len() as u64 != revision.byte_length {
            return Err(BlobError::HashMismatch.into());
        }
        Ok(FilePreview::Image {
            mime_type: revision.mime_type.clone(),
            bytes,
        })
    }

    fn tree_conflict_choices(
        &self,
        projection: &FileTreeProjection,
        conflict: &ConflictRecord,
    ) -> Vec<FileConflictChoiceView> {
        let entries = || {
            let mut entries = Vec::new();
            for node in projection.root.values() {
                flatten_node(node, None, projection, &mut entries);
            }
            entries
        };
        match conflict.kind {
            ConflictKind::ConcurrentCreate => conflict
                .competing_revision_ids
                .iter()
                .filter_map(|candidate_id| {
                    let operation = self.authority.operation(candidate_id)?;
                    let (parent_node_id, name, kind) = match &operation.operation.body {
                        FileOperationBody::CreateFile {
                            parent_node_id,
                            name,
                            ..
                        } => (
                            parent_node_id.as_deref(),
                            name.as_str(),
                            FileConflictChoiceKind::File,
                        ),
                        FileOperationBody::CreateDirectory {
                            parent_node_id,
                            name,
                        } => (
                            parent_node_id.as_deref(),
                            name.as_str(),
                            FileConflictChoiceKind::Directory,
                        ),
                        _ => return None,
                    };
                    let selected = self
                        .authority
                        .node_location(&operation.operation.node_id)
                        .is_some_and(|(current_parent, current_name)| {
                            current_parent == parent_node_id && current_name == name
                        });
                    Some(FileConflictChoiceView {
                        candidate_id: candidate_id.clone(),
                        node_id: operation.operation.node_id.clone(),
                        kind,
                        selected,
                        name: name.to_owned(),
                        target_path: None,
                    })
                })
                .collect(),
            ConflictKind::CompetingMove => {
                let entries = entries();
                conflict
                    .competing_revision_ids
                    .iter()
                    .filter_map(|candidate_id| {
                        let operation = self.authority.operation(candidate_id)?;
                        let FileOperationBody::MoveNode {
                            new_parent_node_id,
                            new_name,
                        } = &operation.operation.body
                        else {
                            return None;
                        };
                        let selected = self.authority.node_location(&conflict.node_id).is_some_and(
                            |(current_parent, current_name)| {
                                current_parent == new_parent_node_id.as_deref()
                                    && current_name == new_name
                            },
                        );
                        let target_path = entry_path(&entries, new_parent_node_id.as_deref())
                            .map_or_else(
                                || new_name.clone(),
                                |parent| format!("{parent}/{new_name}"),
                            );
                        Some(FileConflictChoiceView {
                            candidate_id: candidate_id.clone(),
                            node_id: conflict.node_id.clone(),
                            kind: FileConflictChoiceKind::Move,
                            selected,
                            name: new_name.clone(),
                            target_path: Some(target_path),
                        })
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    pub fn bind_root(
        &mut self,
        root: impl AsRef<Path>,
        selection: RootSelection,
    ) -> Result<(), WorkspaceFileRuntimeError> {
        if self.root.is_some() {
            return self.replace_root(root, selection);
        }
        self.root = Some(LocalRootBinding::bind(
            &self.store,
            root,
            selection,
            &self.authority.projection(),
            self.authority.blob_store(),
        )?);
        Ok(())
    }

    pub fn replace_root(
        &mut self,
        root: impl AsRef<Path>,
        selection: RootSelection,
    ) -> Result<(), WorkspaceFileRuntimeError> {
        match self.root.as_mut() {
            Some(binding) => binding.replace_root(
                root,
                selection,
                &self.authority.projection(),
                self.authority.blob_store(),
            )?,
            None => return self.bind_root(root, selection),
        }
        Ok(())
    }

    pub fn repair_root(&mut self) -> Result<(), WorkspaceFileRuntimeError> {
        let root = self
            .root
            .as_mut()
            .ok_or(WorkspaceFileRuntimeError::NotFound)?;
        root.repair(&self.authority.projection(), self.authority.blob_store())?;
        Ok(())
    }

    pub fn unbind_root(&mut self) -> Result<(), WorkspaceFileRuntimeError> {
        self.store.clear_local_root_binding()?;
        self.root = None;
        Ok(())
    }

    pub fn create_markdown_file(
        &mut self,
        parent_node_id: &str,
        name: &str,
        markdown: &str,
    ) -> Result<MarkdownFileView, WorkspaceFileRuntimeError> {
        validate_markdown(name, markdown)?;
        let projection = self.authority.projection();
        if !is_directory(&projection, parent_node_id) {
            return Err(WorkspaceFileRuntimeError::NotFound);
        }
        let hash = self.authority.blob_store_mut().store(markdown.as_bytes())?;
        let operation = SignedFileOperation::create_file(
            &self.identity,
            &self.workspace_id,
            Some(parent_node_id.to_owned()),
            name,
            hash.as_str(),
            "text/markdown",
            markdown.len() as u64,
            self.authority.causal_frontier(),
        )
        .map_err(AuthorityError::from)?;
        let node_id = operation.operation.node_id.clone();
        let revision_id = operation.operation.operation_id.clone();
        self.commit_operation(operation)?;
        Ok(MarkdownFileView {
            node_id,
            revision_id,
            markdown: markdown.to_owned(),
        })
    }

    pub fn open_markdown_file(
        &self,
        node_id: &str,
        revision_id: &str,
    ) -> Result<MarkdownFileView, WorkspaceFileRuntimeError> {
        let projection = self.authority.projection();
        let entry = find_entry(&projection, node_id).ok_or(WorkspaceFileRuntimeError::NotFound)?;
        if entry.kind != FileEntryKind::Markdown || !entry.editable {
            return Err(WorkspaceFileRuntimeError::NotMarkdown);
        }
        let revision = projection
            .revisions
            .get(revision_id)
            .filter(|revision| revision.node_id == node_id && revision.mime_type == "text/markdown")
            .ok_or(WorkspaceFileRuntimeError::NotFound)?;
        let bytes = self
            .authority
            .blob_store()
            .open(&ContentHash(revision.content_hash.clone()))?;
        if bytes.len() as u64 != revision.byte_length {
            return Err(BlobError::HashMismatch.into());
        }
        let markdown =
            String::from_utf8(bytes).map_err(|_| WorkspaceFileRuntimeError::NotMarkdown)?;
        Ok(MarkdownFileView {
            node_id: node_id.to_owned(),
            revision_id: revision_id.to_owned(),
            markdown,
        })
    }

    pub fn replace_markdown_file(
        &mut self,
        node_id: &str,
        base_revision_id: &str,
        markdown: &str,
    ) -> Result<MarkdownFileView, WorkspaceFileRuntimeError> {
        validate_markdown("file.md", markdown)?;
        let projection = self.authority.projection();
        let entry = find_entry(&projection, node_id).ok_or(WorkspaceFileRuntimeError::NotFound)?;
        if entry.kind != FileEntryKind::Markdown || !entry.editable {
            return Err(WorkspaceFileRuntimeError::NotMarkdown);
        }
        if entry.current_revision_id.as_deref() != Some(base_revision_id) {
            return Err(WorkspaceFileRuntimeError::StaleRevision);
        }
        let hash = self.authority.blob_store_mut().store(markdown.as_bytes())?;
        let operation = SignedFileOperation::replace_file_revision(
            &self.identity,
            &self.workspace_id,
            node_id,
            base_revision_id,
            hash.as_str(),
            "text/markdown",
            markdown.len() as u64,
            self.authority.causal_frontier(),
        )
        .map_err(AuthorityError::from)?;
        let revision_id = operation.operation.operation_id.clone();
        self.commit_operation(operation)?;
        Ok(MarkdownFileView {
            node_id: node_id.to_owned(),
            revision_id,
            markdown: markdown.to_owned(),
        })
    }

    pub fn resolve_conflict(
        &mut self,
        record_id: &str,
        chosen_revision_id: Option<String>,
    ) -> Result<(), WorkspaceFileRuntimeError> {
        let conflict = self
            .authority
            .projection()
            .conflicts
            .into_iter()
            .find(|conflict| conflict.record_id == record_id && !conflict.resolved)
            .ok_or(WorkspaceFileRuntimeError::NotFound)?;
        if chosen_revision_id
            .as_ref()
            .is_some_and(|revision_id| !conflict.competing_revision_ids.contains(revision_id))
        {
            return Err(WorkspaceFileRuntimeError::NotFound);
        }
        let operation = SignedFileOperation::resolve_conflict(
            &self.identity,
            &self.workspace_id,
            conflict.node_id,
            record_id,
            chosen_revision_id,
            self.authority.causal_frontier(),
        )
        .map_err(AuthorityError::from)?;
        self.commit_operation(operation)
    }

    pub fn poll_root_changes(&mut self) -> Result<usize, WorkspaceFileRuntimeError> {
        let Some(mut staged_root) = self.root.clone() else {
            return Ok(0);
        };
        let changes = match staged_root.poll_changes() {
            Ok(changes) => changes,
            Err(error) => {
                self.root = Some(staged_root);
                return Err(error.into());
            }
        };
        if changes.is_empty() {
            self.root = Some(staged_root);
            return Ok(0);
        }

        let mut staged_authority = self.authority.clone();
        let causal_frontier = staged_authority.causal_frontier();
        let operations = staged_root.author_changes(
            &self.identity,
            &self.workspace_id,
            &changes,
            staged_authority.blob_store_mut(),
            causal_frontier,
        )?;
        for operation in &operations {
            staged_authority.apply(operation, &self.membership)?;
        }
        self.store.record_file_operations(&operations)?;

        let count = operations.len();
        self.pending_announcements.extend(
            operations
                .iter()
                .map(|operation| operation.operation.operation_id.clone()),
        );
        self.authority = staged_authority;
        self.root = Some(staged_root);
        if let Some(root) = self.root.as_mut() {
            let _ = root.repair(&self.authority.projection(), self.authority.blob_store());
        }
        Ok(count)
    }

    pub fn recovery_service(&self) -> Result<FileRecoveryService, WorkspaceFileRuntimeError> {
        let mut service = FileRecoveryService::new(&self.workspace_id);
        service.set_members(
            self.membership
                .members
                .iter()
                .map(|member| member.public_identity.clone()),
        );
        for operation in self.store.file_operations()? {
            let operation_id = operation.operation.operation_id.clone();
            service.insert_operation(
                operation_id,
                operation.encode().map_err(AuthorityError::from)?,
            );
            let content_hash = match &operation.operation.body {
                FileOperationBody::CreateFile { content_hash, .. }
                | FileOperationBody::ReplaceFileRevision { content_hash, .. } => Some(content_hash),
                _ => None,
            };
            if let Some(content_hash) = content_hash {
                if let Ok(bytes) = self
                    .authority
                    .blob_store()
                    .open(&ContentHash(content_hash.clone()))
                {
                    if service.insert_blob(bytes) != *content_hash {
                        return Err(BlobError::HashMismatch.into());
                    }
                }
            }
        }
        Ok(service)
    }

    #[must_use]
    pub fn take_pending_announcements(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_announcements)
    }

    fn commit_operation(
        &mut self,
        operation: SignedFileOperation,
    ) -> Result<(), WorkspaceFileRuntimeError> {
        let mut staged_authority = self.authority.clone();
        staged_authority.apply(&operation, &self.membership)?;
        self.store.record_file_operation(&operation)?;

        self.pending_announcements
            .push(operation.operation.operation_id.clone());
        self.authority = staged_authority;
        if let Some(root) = self.root.as_mut() {
            let _ = root.repair(&self.authority.projection(), self.authority.blob_store());
        }
        Ok(())
    }
}

fn validate_markdown(name: &str, markdown: &str) -> Result<(), WorkspaceFileRuntimeError> {
    if !name.to_ascii_lowercase().ends_with(".md") {
        return Err(WorkspaceFileRuntimeError::InvalidName);
    }
    if markdown.len() > MAX_MARKDOWN_BYTES {
        return Err(WorkspaceFileRuntimeError::TooLarge);
    }
    Ok(())
}

fn flatten_node(
    node: &TreeNode,
    parent_node_id: Option<&str>,
    projection: &FileTreeProjection,
    entries: &mut Vec<FileTreeEntry>,
) {
    let (kind, current_revision_id) = match node {
        TreeNode::Directory { .. } => (FileEntryKind::Directory, None),
        TreeNode::File {
            current_revision_id,
            ..
        } => {
            let kind = projection.revisions.get(current_revision_id).map_or(
                FileEntryKind::Binary,
                |revision| {
                    if revision.mime_type == "text/markdown" && node.name().ends_with(".md") {
                        FileEntryKind::Markdown
                    } else {
                        FileEntryKind::Binary
                    }
                },
            );
            (kind, Some(current_revision_id.clone()))
        }
    };
    entries.push(FileTreeEntry {
        node_id: node.node_id().to_owned(),
        parent_node_id: parent_node_id.map(ToOwned::to_owned),
        name: node.name().to_owned(),
        kind,
        current_revision_id,
        editable: !node.name().contains(".resonance-conflict-"),
    });
    if let TreeNode::Directory {
        node_id, children, ..
    } = node
    {
        for child in children.values() {
            flatten_node(child, Some(node_id), projection, entries);
        }
    }
}

fn find_entry(projection: &FileTreeProjection, node_id: &str) -> Option<FileTreeEntry> {
    let mut entries = Vec::new();
    for node in projection.root.values() {
        flatten_node(node, None, projection, &mut entries);
    }
    entries.into_iter().find(|entry| entry.node_id == node_id)
}

fn is_directory(projection: &FileTreeProjection, node_id: &str) -> bool {
    find_entry(projection, node_id).is_some_and(|entry| entry.kind == FileEntryKind::Directory)
}

fn conflict_view(
    conflict: ConflictRecord,
    resolution_candidate_ids: Vec<String>,
    reviewable_revision_ids: Vec<String>,
    deletion_operation_id: Option<String>,
    tree_choices: Vec<FileConflictChoiceView>,
) -> FileConflictView {
    FileConflictView {
        record_id: conflict.record_id,
        node_id: conflict.node_id,
        kind: conflict.kind,
        competing_revision_ids: conflict.competing_revision_ids,
        resolution_candidate_ids,
        reviewable_revision_ids,
        deletion_operation_id,
        tree_choices,
    }
}

fn entry_path(entries: &[FileTreeEntry], node_id: Option<&str>) -> Option<String> {
    let node_id = node_id?;
    let entry = entries.iter().find(|entry| entry.node_id == node_id)?;
    Some(
        entry_path(entries, entry.parent_node_id.as_deref()).map_or_else(
            || entry.name.clone(),
            |parent| format!("{parent}/{}", entry.name),
        ),
    )
}

fn is_previewable_image(mime_type: &str) -> bool {
    matches!(
        mime_type,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/bmp"
    )
}
