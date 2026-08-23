//! Transport-independent workspace file authority.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    membership_log::MembershipProjection,
    workspace_files::{
        blobs::{BlobError, ContentHash, WorkspaceBlobStore},
        merge::{merge_markdown, MergeResult},
        paths::{PathError, PortablePath},
        projection::{ConflictKind, ConflictRecord, FileRevision, FileTreeProjection, TreeNode},
        FileOperationBody, FileOperationError, SignedFileOperation, FILE_OPERATION_VERSION,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum AuthorityError {
    FileOperation(FileOperationError),
    Blob(BlobError),
    Path(PathError),
    WrongWorkspace,
    UnsupportedVersion,
    InvalidIdentifier,
    OperationIdConflict,
    NonMember,
    DirectoryNotEmpty,
    NotADirectory,
    NotFound,
    AlreadyExists,
    InvalidResolution,
}

impl std::fmt::Display for AuthorityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileOperation(error) => write!(formatter, "{error}"),
            Self::Blob(error) => write!(formatter, "{error}"),
            Self::Path(error) => write!(formatter, "{error}"),
            Self::WrongWorkspace => formatter.write_str("operation belongs to another workspace"),
            Self::UnsupportedVersion => {
                formatter.write_str("file operation version is unsupported")
            }
            Self::InvalidIdentifier => {
                formatter.write_str("file operation contains an invalid identifier")
            }
            Self::OperationIdConflict => {
                formatter.write_str("operation ID was reused with different bytes")
            }
            Self::NonMember => formatter.write_str("operation signer is not a workspace member"),
            Self::DirectoryNotEmpty => formatter.write_str("directory is not empty"),
            Self::NotADirectory => formatter.write_str("parent node is not a directory"),
            Self::NotFound => formatter.write_str("file-authority record was not found"),
            Self::AlreadyExists => {
                formatter.write_str("a node already occupies that portable name")
            }
            Self::InvalidResolution => {
                formatter.write_str("conflict resolution does not name a competing revision")
            }
        }
    }
}

impl std::error::Error for AuthorityError {}

impl From<FileOperationError> for AuthorityError {
    fn from(error: FileOperationError) -> Self {
        Self::FileOperation(error)
    }
}

impl From<BlobError> for AuthorityError {
    fn from(error: BlobError) -> Self {
        Self::Blob(error)
    }
}

impl From<PathError> for AuthorityError {
    fn from(error: PathError) -> Self {
        Self::Path(error)
    }
}

#[derive(Clone)]
pub struct WorkspaceFileAuthority {
    workspace_id: String,
    operations: BTreeMap<String, SignedFileOperation>,
    projected_operations: BTreeSet<String>,
    nodes: BTreeMap<String, NodeRecord>,
    children: BTreeMap<Option<String>, BTreeMap<String, String>>,
    revisions: BTreeMap<String, FileRevision>,
    conflicts: BTreeMap<String, ConflictRecord>,
    blobs: WorkspaceBlobStore,
}

#[derive(Clone, Debug)]
struct NodeRecord {
    name: String,
    parent_node_id: Option<String>,
    kind: NodeKind,
    tombstoned: bool,
    current_revision_id: Option<String>,
    created_by: String,
    last_move_operation_id: Option<String>,
    tombstone_operation_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NodeKind {
    Directory,
    File,
}

impl WorkspaceFileAuthority {
    #[must_use]
    pub fn new(workspace_id: impl Into<String>) -> Self {
        Self::with_blob_store(workspace_id, WorkspaceBlobStore::new())
    }

    #[must_use]
    pub fn with_blob_store(workspace_id: impl Into<String>, blobs: WorkspaceBlobStore) -> Self {
        Self {
            workspace_id: workspace_id.into(),
            operations: BTreeMap::new(),
            projected_operations: BTreeSet::new(),
            nodes: BTreeMap::new(),
            children: BTreeMap::new(),
            revisions: BTreeMap::new(),
            conflicts: BTreeMap::new(),
            blobs,
        }
    }

    pub fn apply(
        &mut self,
        operation: &SignedFileOperation,
        membership: &MembershipProjection,
    ) -> Result<(), AuthorityError> {
        self.validate_envelope(operation, membership)?;
        let operation_id = operation.operation.operation_id.clone();
        if let Some(existing) = self.operations.get(&operation_id) {
            return if existing == operation {
                Ok(())
            } else {
                Err(AuthorityError::OperationIdConflict)
            };
        }

        self.operations
            .insert(operation_id.clone(), operation.clone());
        if let Err(error) = self.rebuild_projection() {
            self.operations.remove(&operation_id);
            self.rebuild_projection()
                .expect("previously valid projection must rebuild");
            return Err(error);
        }
        Ok(())
    }

    pub fn replay(
        &mut self,
        operations: &[SignedFileOperation],
        membership: &MembershipProjection,
    ) -> Result<(), AuthorityError> {
        for operation in operations {
            self.validate_envelope(operation, membership)?;
            let operation_id = operation.operation.operation_id.clone();
            if let Some(existing) = self.operations.insert(operation_id, operation.clone()) {
                if existing != *operation {
                    return Err(AuthorityError::OperationIdConflict);
                }
            }
        }
        self.rebuild_projection()
    }

    #[must_use]
    pub fn projection(&self) -> FileTreeProjection {
        let mut root = BTreeMap::new();
        if let Some(children) = self.children.get(&None) {
            for node_id in children.values() {
                if let Some(node) = self.build_tree(node_id) {
                    root.insert(node.name().to_owned(), node);
                }
            }
        }
        self.add_conflict_artifacts(&mut root);
        FileTreeProjection {
            root,
            revisions: self.revisions.clone(),
            conflicts: self.conflicts.values().cloned().collect(),
        }
    }

    #[must_use]
    pub fn blob_store(&self) -> &WorkspaceBlobStore {
        &self.blobs
    }

    #[must_use]
    pub fn blob_store_mut(&mut self) -> &mut WorkspaceBlobStore {
        &mut self.blobs
    }

    #[must_use]
    pub fn applied_operation_ids(&self) -> &BTreeSet<String> {
        &self.projected_operations
    }

    #[must_use]
    pub fn causal_frontier(&self) -> Vec<String> {
        let parents = self
            .operations
            .values()
            .flat_map(|operation| operation.operation.causal_parents.iter())
            .collect::<BTreeSet<_>>();
        self.projected_operations
            .iter()
            .filter(|operation_id| !parents.contains(operation_id))
            .cloned()
            .collect()
    }

    fn validate_envelope(
        &self,
        operation: &SignedFileOperation,
        membership: &MembershipProjection,
    ) -> Result<(), AuthorityError> {
        operation.verify()?;
        if operation.operation.version != FILE_OPERATION_VERSION {
            return Err(AuthorityError::UnsupportedVersion);
        }
        if operation.operation.workspace_id != self.workspace_id {
            return Err(AuthorityError::WrongWorkspace);
        }
        if !valid_id(&operation.operation.operation_id)
            || !valid_id(&operation.operation.node_id)
            || operation
                .operation
                .causal_parents
                .iter()
                .any(|parent| !valid_id(parent))
        {
            return Err(AuthorityError::InvalidIdentifier);
        }
        if !membership.contains(&public_identity_text(&operation.operation.signer)) {
            return Err(AuthorityError::NonMember);
        }
        Ok(())
    }

    fn rebuild_projection(&mut self) -> Result<(), AuthorityError> {
        self.projected_operations.clear();
        self.nodes.clear();
        self.children.clear();
        self.revisions.clear();
        self.conflicts.clear();

        loop {
            let ready = self
                .operations
                .iter()
                .filter(|(id, operation)| {
                    !self.projected_operations.contains(*id)
                        && operation
                            .operation
                            .causal_parents
                            .iter()
                            .all(|parent| self.projected_operations.contains(parent))
                })
                .map(|(id, operation)| (id.clone(), operation.clone()))
                .collect::<Vec<_>>();
            if ready.is_empty() {
                break;
            }
            for (operation_id, operation) in ready {
                self.project_operation(&operation)?;
                self.projected_operations.insert(operation_id);
            }
        }
        Ok(())
    }

    fn project_operation(&mut self, signed: &SignedFileOperation) -> Result<(), AuthorityError> {
        let operation = &signed.operation;
        match &operation.body {
            FileOperationBody::CreateDirectory {
                parent_node_id,
                name,
            } => self.create_directory(operation, parent_node_id.as_deref(), name),
            FileOperationBody::CreateFile {
                parent_node_id,
                name,
                content_hash,
                mime_type,
                byte_length,
            } => self.create_file(
                operation,
                parent_node_id.as_deref(),
                name,
                content_hash,
                mime_type,
                *byte_length,
            ),
            FileOperationBody::ReplaceFileRevision {
                base_revision_id,
                content_hash,
                mime_type,
                byte_length,
            } => self.replace_file(
                operation,
                base_revision_id,
                content_hash,
                mime_type,
                *byte_length,
            ),
            FileOperationBody::MoveNode {
                new_parent_node_id,
                new_name,
            } => self.move_node(operation, new_parent_node_id.as_deref(), new_name),
            FileOperationBody::TombstoneNode => self.tombstone_node(operation),
            FileOperationBody::ResolveConflict {
                conflict_record_id,
                chosen_revision_id,
            } => self.resolve_conflict(
                &operation.node_id,
                conflict_record_id,
                chosen_revision_id.as_deref(),
            ),
        }
    }

    fn create_directory(
        &mut self,
        operation: &crate::workspace_files::FileOperation,
        parent_node_id: Option<&str>,
        name: &str,
    ) -> Result<(), AuthorityError> {
        validate_name(name)?;
        self.require_parent_directory(parent_node_id)?;
        let name = self.available_create_name(operation, parent_node_id, name)?;
        self.insert_child(parent_node_id, &name, &operation.node_id)?;
        self.nodes.insert(
            operation.node_id.clone(),
            NodeRecord {
                name,
                parent_node_id: parent_node_id.map(ToOwned::to_owned),
                kind: NodeKind::Directory,
                tombstoned: false,
                current_revision_id: None,
                created_by: operation.operation_id.clone(),
                last_move_operation_id: None,
                tombstone_operation_id: None,
            },
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn create_file(
        &mut self,
        operation: &crate::workspace_files::FileOperation,
        parent_node_id: Option<&str>,
        name: &str,
        content_hash: &str,
        mime_type: &str,
        byte_length: u64,
    ) -> Result<(), AuthorityError> {
        validate_name(name)?;
        self.require_parent_directory(parent_node_id)?;
        let name = self.available_create_name(operation, parent_node_id, name)?;
        self.insert_child(parent_node_id, &name, &operation.node_id)?;
        let revision_id = operation.operation_id.clone();
        self.revisions.insert(
            revision_id.clone(),
            FileRevision {
                node_id: operation.node_id.clone(),
                revision_id: revision_id.clone(),
                base_revision_id: None,
                content_hash: content_hash.to_owned(),
                mime_type: mime_type.to_owned(),
                byte_length,
                signer: operation.signer,
            },
        );
        self.nodes.insert(
            operation.node_id.clone(),
            NodeRecord {
                name,
                parent_node_id: parent_node_id.map(ToOwned::to_owned),
                kind: NodeKind::File,
                tombstoned: false,
                current_revision_id: Some(revision_id),
                created_by: operation.operation_id.clone(),
                last_move_operation_id: None,
                tombstone_operation_id: None,
            },
        );
        Ok(())
    }

    fn replace_file(
        &mut self,
        operation: &crate::workspace_files::FileOperation,
        base_revision_id: &str,
        content_hash: &str,
        mime_type: &str,
        byte_length: u64,
    ) -> Result<(), AuthorityError> {
        let node = self
            .nodes
            .get(&operation.node_id)
            .cloned()
            .ok_or(AuthorityError::NotFound)?;
        if node.kind != NodeKind::File {
            return Err(AuthorityError::NotFound);
        }
        if !self.revisions.contains_key(base_revision_id) {
            return Err(AuthorityError::NotFound);
        }
        let incoming_revision_id = operation.operation_id.clone();
        self.revisions.insert(
            incoming_revision_id.clone(),
            FileRevision {
                node_id: operation.node_id.clone(),
                revision_id: incoming_revision_id.clone(),
                base_revision_id: Some(base_revision_id.to_owned()),
                content_hash: content_hash.to_owned(),
                mime_type: mime_type.to_owned(),
                byte_length,
                signer: operation.signer,
            },
        );
        let current_revision_id = node.current_revision_id.ok_or(AuthorityError::NotFound)?;
        if node.tombstoned {
            let tombstone = node
                .tombstone_operation_id
                .clone()
                .ok_or(AuthorityError::NotFound)?;
            if operation
                .causal_parents
                .iter()
                .any(|parent| parent == &tombstone)
            {
                return Err(AuthorityError::NotFound);
            }
            let mut competing = vec![incoming_revision_id.clone(), tombstone];
            competing.sort();
            self.record_conflict(&operation.node_id, ConflictKind::DeleteEdit, &competing)?;
            let existing = self
                .nodes
                .get(&operation.node_id)
                .expect("node was checked")
                .clone();
            self.insert_child(
                existing.parent_node_id.as_deref(),
                &existing.name,
                &operation.node_id,
            )?;
            let restored = self
                .nodes
                .get_mut(&operation.node_id)
                .expect("node was checked");
            restored.tombstoned = false;
            restored.tombstone_operation_id = None;
            restored.current_revision_id = Some(incoming_revision_id);
            return Ok(());
        }
        let selected_revision = if current_revision_id == base_revision_id {
            incoming_revision_id
        } else {
            self.merge_or_conflict(
                &operation.node_id,
                base_revision_id,
                &current_revision_id,
                &incoming_revision_id,
            )?
        };
        self.nodes
            .get_mut(&operation.node_id)
            .expect("node was checked")
            .current_revision_id = Some(selected_revision);
        Ok(())
    }

    fn merge_or_conflict(
        &mut self,
        node_id: &str,
        base_revision_id: &str,
        current_revision_id: &str,
        incoming_revision_id: &str,
    ) -> Result<String, AuthorityError> {
        let base = self.revisions.get(base_revision_id).cloned();
        let current = self
            .revisions
            .get(current_revision_id)
            .cloned()
            .ok_or(AuthorityError::NotFound)?;
        let incoming = self
            .revisions
            .get(incoming_revision_id)
            .cloned()
            .ok_or(AuthorityError::NotFound)?;
        if let Some(base) = base {
            if current.mime_type == "text/markdown" && incoming.mime_type == "text/markdown" {
                let base_bytes = self.blobs.open(&ContentHash(base.content_hash))?;
                let current_bytes = self.blobs.open(&ContentHash(current.content_hash))?;
                let incoming_bytes = self.blobs.open(&ContentHash(incoming.content_hash))?;
                if let (Ok(base), Ok(current), Ok(incoming)) = (
                    String::from_utf8(base_bytes),
                    String::from_utf8(current_bytes),
                    String::from_utf8(incoming_bytes),
                ) {
                    if let MergeResult::Merged(content) = merge_markdown(&base, &current, &incoming)
                    {
                        let hash = self.blobs.store(content.as_bytes())?;
                        let revision_id = derived_id(
                            b"resonance.merged-revision.v1\0",
                            &[
                                base_revision_id,
                                current_revision_id,
                                incoming_revision_id,
                                hash.as_str(),
                            ],
                        );
                        self.revisions.insert(
                            revision_id.clone(),
                            FileRevision {
                                node_id: node_id.to_owned(),
                                revision_id: revision_id.clone(),
                                base_revision_id: Some(base_revision_id.to_owned()),
                                content_hash: hash.as_str().to_owned(),
                                mime_type: "text/markdown".to_owned(),
                                byte_length: content.len() as u64,
                                signer: [0; 32],
                            },
                        );
                        return Ok(revision_id);
                    }
                }
            }
        }

        let kind = if current.mime_type == "text/markdown" && incoming.mime_type == "text/markdown"
        {
            ConflictKind::MarkdownOverlap
        } else {
            ConflictKind::BinaryCollision
        };
        let mut competing = vec![
            current_revision_id.to_owned(),
            incoming_revision_id.to_owned(),
        ];
        competing.sort();
        self.record_conflict(node_id, kind, &competing)?;
        Ok(competing[0].clone())
    }

    fn move_node(
        &mut self,
        operation: &crate::workspace_files::FileOperation,
        new_parent_node_id: Option<&str>,
        new_name: &str,
    ) -> Result<(), AuthorityError> {
        let node_id = &operation.node_id;
        validate_name(new_name)?;
        self.require_parent_directory(new_parent_node_id)?;
        let existing = self
            .nodes
            .get(node_id)
            .cloned()
            .ok_or(AuthorityError::NotFound)?;
        if existing.tombstoned {
            return Err(AuthorityError::NotFound);
        }
        if let Some(previous_move) = existing.last_move_operation_id.as_deref() {
            if !operation
                .causal_parents
                .iter()
                .any(|parent| parent == previous_move)
            {
                let mut competing = vec![previous_move.to_owned(), operation.operation_id.clone()];
                competing.sort();
                self.record_conflict(node_id, ConflictKind::CompetingMove, &competing)?;
                return Ok(());
            }
        }
        self.remove_child(existing.parent_node_id.as_deref(), &existing.name);
        if let Err(error) = self.insert_child(new_parent_node_id, new_name, node_id) {
            self.insert_child(existing.parent_node_id.as_deref(), &existing.name, node_id)
                .expect("existing location must remain valid");
            return Err(error);
        }
        let node = self.nodes.get_mut(node_id).expect("node was checked");
        node.parent_node_id = new_parent_node_id.map(ToOwned::to_owned);
        node.name = new_name.to_owned();
        node.last_move_operation_id = Some(operation.operation_id.clone());
        Ok(())
    }

    fn tombstone_node(
        &mut self,
        operation: &crate::workspace_files::FileOperation,
    ) -> Result<(), AuthorityError> {
        let node_id = &operation.node_id;
        let existing = self
            .nodes
            .get(node_id)
            .cloned()
            .ok_or(AuthorityError::NotFound)?;
        if existing.kind == NodeKind::Directory
            && self
                .children
                .get(&Some(node_id.to_owned()))
                .is_some_and(|children| !children.is_empty())
        {
            return Err(AuthorityError::DirectoryNotEmpty);
        }
        if existing.kind == NodeKind::File {
            if let Some(current_revision) = existing.current_revision_id.as_deref() {
                if current_revision != existing.created_by
                    && !operation
                        .causal_parents
                        .iter()
                        .any(|parent| parent == current_revision)
                {
                    let mut competing =
                        vec![current_revision.to_owned(), operation.operation_id.clone()];
                    competing.sort();
                    self.record_conflict(node_id, ConflictKind::DeleteEdit, &competing)?;
                    return Ok(());
                }
            }
        }
        self.remove_child(existing.parent_node_id.as_deref(), &existing.name);
        let node = self.nodes.get_mut(node_id).expect("node was checked");
        node.tombstoned = true;
        node.tombstone_operation_id = Some(operation.operation_id.clone());
        Ok(())
    }

    fn resolve_conflict(
        &mut self,
        node_id: &str,
        conflict_record_id: &str,
        chosen_revision_id: Option<&str>,
    ) -> Result<(), AuthorityError> {
        let conflict = self
            .conflicts
            .get(conflict_record_id)
            .cloned()
            .ok_or(AuthorityError::NotFound)?;
        if conflict.node_id != node_id
            || chosen_revision_id.is_some_and(|revision_id| {
                !conflict
                    .competing_revision_ids
                    .iter()
                    .any(|candidate| candidate == revision_id)
            })
        {
            return Err(AuthorityError::InvalidResolution);
        }

        if conflict.kind == ConflictKind::ConcurrentCreate {
            self.resolve_concurrent_create(&conflict, chosen_revision_id)?;
        } else if let Some(revision_id) = chosen_revision_id {
            let revision = self
                .revisions
                .get(revision_id)
                .filter(|revision| revision.node_id == node_id)
                .ok_or(AuthorityError::InvalidResolution)?;
            self.nodes
                .get_mut(node_id)
                .ok_or(AuthorityError::NotFound)?
                .current_revision_id = Some(revision.revision_id.clone());
        }
        self.conflicts
            .get_mut(conflict_record_id)
            .expect("conflict was checked")
            .resolved = true;
        Ok(())
    }

    fn resolve_concurrent_create(
        &mut self,
        conflict: &ConflictRecord,
        chosen_operation_id: Option<&str>,
    ) -> Result<(), AuthorityError> {
        let candidates = self
            .nodes
            .iter()
            .filter(|(_, node)| {
                conflict
                    .competing_revision_ids
                    .iter()
                    .any(|operation_id| operation_id == &node.created_by)
            })
            .map(|(node_id, node)| (node_id.clone(), node.clone()))
            .collect::<Vec<_>>();
        let selected = chosen_operation_id
            .and_then(|operation_id| {
                candidates
                    .iter()
                    .find(|(_, node)| node.created_by == operation_id)
            })
            .or_else(|| {
                candidates
                    .iter()
                    .find(|(_, node)| !node.name.contains(".resonance-conflict-"))
            })
            .cloned()
            .ok_or(AuthorityError::InvalidResolution)?;
        let selected_operation = self
            .operations
            .get(&selected.1.created_by)
            .ok_or(AuthorityError::NotFound)?;
        let (requested_parent, requested_name) = match &selected_operation.operation.body {
            FileOperationBody::CreateDirectory {
                parent_node_id,
                name,
            }
            | FileOperationBody::CreateFile {
                parent_node_id,
                name,
                ..
            } => (parent_node_id.clone(), name.clone()),
            _ => return Err(AuthorityError::InvalidResolution),
        };

        for (candidate_node_id, candidate) in &candidates {
            self.remove_child(candidate.parent_node_id.as_deref(), &candidate.name);
            self.nodes
                .get_mut(candidate_node_id)
                .expect("candidate node was collected")
                .tombstoned = candidate_node_id != &selected.0;
        }
        let selected_node = self
            .nodes
            .get_mut(&selected.0)
            .expect("selected node was collected");
        selected_node.parent_node_id = requested_parent.clone();
        selected_node.name = requested_name.clone();
        selected_node.tombstoned = false;
        self.insert_child(requested_parent.as_deref(), &requested_name, &selected.0)?;
        Ok(())
    }

    fn available_create_name(
        &mut self,
        operation: &crate::workspace_files::FileOperation,
        parent_node_id: Option<&str>,
        requested_name: &str,
    ) -> Result<String, AuthorityError> {
        let sibling = self
            .children
            .get(&parent_node_id.map(ToOwned::to_owned))
            .and_then(|siblings| siblings.get(&requested_name.to_lowercase()))
            .and_then(|node_id| self.nodes.get(node_id))
            .cloned();
        let Some(sibling) = sibling else {
            return Ok(requested_name.to_owned());
        };
        if operation
            .causal_parents
            .iter()
            .any(|parent| parent == &sibling.created_by)
        {
            return Err(AuthorityError::AlreadyExists);
        }
        let mut competing = vec![sibling.created_by, operation.operation_id.clone()];
        competing.sort();
        self.record_conflict(
            &operation.node_id,
            ConflictKind::ConcurrentCreate,
            &competing,
        )?;
        Ok(revision_conflict_name(
            requested_name,
            &operation.operation_id,
        ))
    }

    fn record_conflict(
        &mut self,
        node_id: &str,
        kind: ConflictKind,
        competing: &[String],
    ) -> Result<(), AuthorityError> {
        let mut competing = competing.to_vec();
        competing.sort();
        competing.dedup();
        let kind_name = format!("{kind:?}");
        let joined = competing.join(":");
        let record_id = derived_id(
            b"resonance.file-conflict.v1\0",
            &[node_id, &kind_name, &joined],
        );
        if matches!(kind, ConflictKind::DeleteEdit | ConflictKind::CompetingMove) {
            let notice = match kind {
                ConflictKind::DeleteEdit => {
                    "Resonance preserved a deletion that happened concurrently with an edit.\n"
                }
                ConflictKind::CompetingMove => {
                    "Resonance preserved a competing move for this workspace entry.\n"
                }
                _ => unreachable!(),
            };
            let content_hash = self.blobs.store(notice.as_bytes())?;
            let revision_id = conflict_notice_revision_id(&record_id);
            self.revisions
                .entry(revision_id.clone())
                .or_insert(FileRevision {
                    node_id: conflict_artifact_node_id(&record_id, &revision_id),
                    revision_id,
                    base_revision_id: None,
                    content_hash: content_hash.as_str().to_owned(),
                    mime_type: "text/plain".to_owned(),
                    byte_length: notice.len() as u64,
                    signer: [0; 32],
                });
        }
        self.conflicts
            .entry(record_id.clone())
            .or_insert(ConflictRecord {
                record_id,
                node_id: node_id.to_owned(),
                kind,
                competing_revision_ids: competing,
                resolved: false,
            });
        Ok(())
    }

    fn add_conflict_artifacts(&self, root: &mut BTreeMap<String, TreeNode>) {
        for conflict in self
            .conflicts
            .values()
            .filter(|conflict| !conflict.resolved)
        {
            let Some(node) = self.nodes.get(&conflict.node_id) else {
                continue;
            };
            match conflict.kind {
                ConflictKind::MarkdownOverlap | ConflictKind::BinaryCollision => {
                    for revision_id in &conflict.competing_revision_ids {
                        if node.current_revision_id.as_deref() == Some(revision_id)
                            || !self.revisions.contains_key(revision_id)
                        {
                            continue;
                        }
                        insert_projected_child(
                            root,
                            node.parent_node_id.as_deref(),
                            TreeNode::File {
                                node_id: conflict_artifact_node_id(
                                    &conflict.record_id,
                                    revision_id,
                                ),
                                name: revision_conflict_name(&node.name, revision_id),
                                current_revision_id: revision_id.clone(),
                            },
                        );
                    }
                }
                ConflictKind::DeleteEdit => {
                    let deletion_id = conflict
                        .competing_revision_ids
                        .iter()
                        .find(|candidate| !self.revisions.contains_key(*candidate));
                    let Some(deletion_id) = deletion_id else {
                        continue;
                    };
                    let revision_id = conflict_notice_revision_id(&conflict.record_id);
                    insert_projected_child(
                        root,
                        node.parent_node_id.as_deref(),
                        TreeNode::File {
                            node_id: conflict_artifact_node_id(&conflict.record_id, &revision_id),
                            name: format!(
                                "{}.resonance-conflict-{}.deleted",
                                node.name,
                                &deletion_id[..8]
                            ),
                            current_revision_id: revision_id,
                        },
                    );
                }
                ConflictKind::CompetingMove => {
                    let revision_id = conflict_notice_revision_id(&conflict.record_id);
                    for operation_id in &conflict.competing_revision_ids {
                        let Some(operation) = self.operations.get(operation_id) else {
                            continue;
                        };
                        let FileOperationBody::MoveNode {
                            new_parent_node_id,
                            new_name,
                        } = &operation.operation.body
                        else {
                            continue;
                        };
                        if node.parent_node_id == *new_parent_node_id && node.name == *new_name {
                            continue;
                        }
                        insert_projected_child(
                            root,
                            new_parent_node_id.as_deref(),
                            TreeNode::File {
                                node_id: conflict_artifact_node_id(
                                    &conflict.record_id,
                                    operation_id,
                                ),
                                name: format!(
                                    "{new_name}.resonance-conflict-{}.move",
                                    &operation_id[..8]
                                ),
                                current_revision_id: revision_id.clone(),
                            },
                        );
                    }
                }
                ConflictKind::ConcurrentCreate => {}
            }
        }
    }

    fn require_parent_directory(&self, parent_node_id: Option<&str>) -> Result<(), AuthorityError> {
        let Some(parent_node_id) = parent_node_id else {
            return Ok(());
        };
        let parent = self
            .nodes
            .get(parent_node_id)
            .ok_or(AuthorityError::NotFound)?;
        if parent.kind != NodeKind::Directory || parent.tombstoned {
            return Err(AuthorityError::NotADirectory);
        }
        Ok(())
    }

    fn insert_child(
        &mut self,
        parent_node_id: Option<&str>,
        name: &str,
        node_id: &str,
    ) -> Result<(), AuthorityError> {
        let siblings = self
            .children
            .entry(parent_node_id.map(ToOwned::to_owned))
            .or_default();
        let folded = name.to_lowercase();
        if siblings.contains_key(&folded) {
            return Err(AuthorityError::AlreadyExists);
        }
        siblings.insert(folded, node_id.to_owned());
        Ok(())
    }

    fn remove_child(&mut self, parent_node_id: Option<&str>, name: &str) {
        if let Some(siblings) = self
            .children
            .get_mut(&parent_node_id.map(ToOwned::to_owned))
        {
            siblings.remove(&name.to_lowercase());
        }
    }

    fn build_tree(&self, node_id: &str) -> Option<TreeNode> {
        let node = self.nodes.get(node_id)?;
        if node.tombstoned {
            return None;
        }
        match node.kind {
            NodeKind::Directory => {
                let mut children = BTreeMap::new();
                if let Some(child_ids) = self.children.get(&Some(node_id.to_owned())) {
                    for child_id in child_ids.values() {
                        if let Some(child) = self.build_tree(child_id) {
                            children.insert(child.name().to_owned(), child);
                        }
                    }
                }
                Some(TreeNode::Directory {
                    node_id: node_id.to_owned(),
                    name: node.name.clone(),
                    children,
                })
            }
            NodeKind::File => Some(TreeNode::File {
                node_id: node_id.to_owned(),
                name: node.name.clone(),
                current_revision_id: node.current_revision_id.clone()?,
            }),
        }
    }
}

fn insert_projected_child(
    root: &mut BTreeMap<String, TreeNode>,
    parent_node_id: Option<&str>,
    child: TreeNode,
) {
    let name = child.name().to_owned();
    if let Some(parent_node_id) = parent_node_id {
        if let Some(children) = projected_children_mut(root, parent_node_id) {
            children.insert(name, child);
        }
    } else {
        root.insert(name, child);
    }
}

fn projected_children_mut<'a>(
    nodes: &'a mut BTreeMap<String, TreeNode>,
    parent_node_id: &str,
) -> Option<&'a mut BTreeMap<String, TreeNode>> {
    for node in nodes.values_mut() {
        let TreeNode::Directory {
            node_id, children, ..
        } = node
        else {
            continue;
        };
        if node_id == parent_node_id {
            return Some(children);
        }
        if let Some(found) = projected_children_mut(children, parent_node_id) {
            return Some(found);
        }
    }
    None
}

fn revision_conflict_name(name: &str, operation_id: &str) -> String {
    let prefix = &operation_id[..8];
    if let Some((stem, extension)) = name.rsplit_once('.') {
        if !stem.is_empty() && !extension.is_empty() {
            return format!("{stem}.resonance-conflict-{prefix}.{extension}");
        }
    }
    format!("{name}.resonance-conflict-{prefix}")
}

fn conflict_notice_revision_id(record_id: &str) -> String {
    derived_id(b"resonance.conflict-notice-revision.v1\0", &[record_id])
}

fn conflict_artifact_node_id(record_id: &str, variant: &str) -> String {
    derived_id(
        b"resonance.conflict-artifact-node.v1\0",
        &[record_id, variant],
    )
}

fn validate_name(name: &str) -> Result<(), AuthorityError> {
    let path = PortablePath::parse(name)?;
    if path.segments().len() != 1 {
        return Err(PathError::SeparatorInName.into());
    }
    Ok(())
}

fn valid_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn derived_id(domain: &[u8], values: &[&str]) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    let mut values = values.to_vec();
    values.sort_unstable();
    for value in values {
        hasher.update(&(value.len() as u64).to_le_bytes());
        hasher.update(value.as_bytes());
    }
    hasher.finalize().to_hex()[..32].to_owned()
}

fn public_identity_text(public_identity: &[u8; 32]) -> String {
    crate::identity::PublicIdentity::from_bytes(*public_identity).to_string()
}
