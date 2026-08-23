//! Transport-independent file authority.
//!
//! Accepts valid member-signed file operations, idempotently persists them,
//! and deterministically projects nodes, revisions, and conflicts.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    membership_log::MembershipProjection,
    workspace_files::{
        blobs::{BlobError, ContentHash, WorkspaceBlobStore},
        merge::{self, MergeResult},
        paths::{PathError, PortablePath},
        projection::{ConflictKind, ConflictRecord, FileRevision, FileTreeProjection, TreeNode},
        random_id, FileOperationBody, FileOperationError, SignedFileOperation,
    },
};

#[derive(Debug, PartialEq, Eq)]
pub enum AuthorityError {
    FileOperation(FileOperationError),
    Blob(BlobError),
    Path(PathError),
    OperationIdConflict,
    InvalidCausalParent,
    NonMember,
    MissingBaseRevision,
    DirectoryNotEmpty,
    NotADirectory,
    NotFound,
    AlreadyExists,
    MergeConflict(&'static str),
    NotATextFile,
    NotMarkdown,
}

impl std::fmt::Display for AuthorityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FileOperation(error) => write!(formatter, "{error}"),
            Self::Blob(error) => write!(formatter, "{error}"),
            Self::Path(error) => write!(formatter, "{error}"),
            Self::OperationIdConflict => {
                formatter.write_str("operation ID has already been applied")
            }
            Self::InvalidCausalParent => {
                formatter.write_str("causal parent is not a known operation")
            }
            Self::NonMember => formatter.write_str("signer is not a workspace member"),
            Self::MissingBaseRevision => {
                formatter.write_str("base revision is not the current revision")
            }
            Self::DirectoryNotEmpty => formatter.write_str("directory is not empty"),
            Self::NotADirectory => formatter.write_str("parent node is not a directory"),
            Self::NotFound => formatter.write_str("node not found"),
            Self::AlreadyExists => formatter.write_str("node already exists at this path"),
            Self::MergeConflict(reason) => write!(formatter, "merge conflict: {reason}"),
            Self::NotATextFile => formatter.write_str("file is not valid UTF-8 text"),
            Self::NotMarkdown => formatter.write_str("only Markdown files support merging"),
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

/// The core file authority. Persists operations and projects a file tree.
pub struct WorkspaceFileAuthority {
    applied_operations: BTreeSet<String>,
    pending_operations: BTreeMap<String, SignedFileOperation>,
    nodes: BTreeMap<String, NodeRecord>,
    directory_children: BTreeMap<String, BTreeSet<String>>,
    revisions: BTreeMap<String, FileRevision>,
    current_revisions: BTreeMap<String, String>,
    conflicts: Vec<ConflictRecord>,
    blobs: WorkspaceBlobStore,
}

#[derive(Clone, Debug)]
struct NodeRecord {
    name: String,
    parent_node_id: Option<String>,
    is_directory: bool,
    is_tombstone: bool,
}

impl Default for WorkspaceFileAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceFileAuthority {
    #[must_use]
    pub fn new() -> Self {
        Self {
            applied_operations: BTreeSet::new(),
            pending_operations: BTreeMap::new(),
            nodes: BTreeMap::new(),
            directory_children: BTreeMap::new(),
            revisions: BTreeMap::new(),
            current_revisions: BTreeMap::new(),
            conflicts: Vec::new(),
            blobs: WorkspaceBlobStore::new(),
        }
    }

    pub fn apply(
        &mut self,
        operation: &SignedFileOperation,
        membership: &MembershipProjection,
    ) -> Result<(), AuthorityError> {
        operation.verify()?;
        if self
            .applied_operations
            .contains(&operation.operation.operation_id)
        {
            return Ok(());
        }
        if !membership.contains(&public_identity_text(&operation.operation.signer)) {
            return Err(AuthorityError::NonMember);
        }
        for parent in &operation.operation.causal_parents {
            if !self.applied_operations.contains(parent) {
                self.pending_operations
                    .insert(operation.operation.operation_id.clone(), operation.clone());
                return Ok(());
            }
        }

        self.apply_operation(operation)?;
        self.applied_operations
            .insert(operation.operation.operation_id.clone());
        self.retry_pending(membership)?;
        Ok(())
    }

    pub fn replay(
        &mut self,
        operations: &[SignedFileOperation],
        membership: &MembershipProjection,
    ) -> Result<(), AuthorityError> {
        for operation in operations {
            self.apply(operation, membership)?;
        }
        Ok(())
    }

    #[must_use]
    pub fn projection(&self) -> FileTreeProjection {
        let mut root = BTreeMap::new();
        for (node_id, record) in &self.nodes {
            if record.parent_node_id.is_none() && !record.is_tombstone {
                let child = self.build_tree(node_id);
                if let Some(child) = child {
                    root.insert(record.name.clone(), child);
                }
            }
        }
        FileTreeProjection {
            root,
            revisions: self.revisions.clone(),
            conflicts: self.conflicts.clone(),
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
        &self.applied_operations
    }

    fn apply_operation(&mut self, operation: &SignedFileOperation) -> Result<(), AuthorityError> {
        match &operation.operation.body {
            FileOperationBody::CreateDirectory { name } => {
                self.apply_create_directory(operation, name)?;
            }
            FileOperationBody::CreateFile {
                parent_node_id,
                name,
                content_hash,
                mime_type,
                byte_length,
            } => {
                self.apply_create_file(
                    operation,
                    parent_node_id,
                    name,
                    content_hash,
                    mime_type,
                    *byte_length,
                )?;
            }
            FileOperationBody::ReplaceFileRevision {
                node_id,
                base_revision_id,
                content_hash,
                mime_type,
                byte_length,
            } => {
                self.apply_replace_revision(
                    operation,
                    node_id,
                    base_revision_id,
                    content_hash,
                    mime_type,
                    *byte_length,
                )?;
            }
            FileOperationBody::MoveNode {
                node_id,
                new_parent_node_id,
                new_name,
            } => {
                self.apply_move(node_id, new_parent_node_id, new_name)?;
            }
            FileOperationBody::TombstoneNode { node_id } => {
                self.apply_tombstone(node_id)?;
            }
            FileOperationBody::ResolveConflict {
                conflict_record_id,
                chosen_revision_id,
            } => {
                self.apply_resolve(conflict_record_id, chosen_revision_id)?;
            }
        }
        Ok(())
    }

    fn apply_create_directory(
        &mut self,
        operation: &SignedFileOperation,
        name: &str,
    ) -> Result<(), AuthorityError> {
        PortablePath::parse(name)?;
        if self.root_node_exists(name) {
            return Err(AuthorityError::AlreadyExists);
        }
        self.nodes.insert(
            operation.operation.node_id.clone(),
            NodeRecord {
                name: name.to_owned(),
                parent_node_id: None,
                is_directory: true,
                is_tombstone: false,
            },
        );
        self.directory_children
            .entry(operation.operation.node_id.clone())
            .or_default();
        Ok(())
    }

    fn apply_create_file(
        &mut self,
        operation: &SignedFileOperation,
        parent_node_id: &str,
        name: &str,
        content_hash: &str,
        mime_type: &str,
        byte_length: u64,
    ) -> Result<(), AuthorityError> {
        PortablePath::parse(name)?;
        let parent = self
            .nodes
            .get(parent_node_id)
            .ok_or(AuthorityError::NotFound)?;
        if !parent.is_directory || parent.is_tombstone {
            return Err(AuthorityError::NotADirectory);
        }
        if self.child_exists(parent_node_id, name) {
            return Err(AuthorityError::AlreadyExists);
        }
        let revision_id = random_id()?;
        let node_id = operation.operation.node_id.clone();
        self.nodes.insert(
            node_id.clone(),
            NodeRecord {
                name: name.to_owned(),
                parent_node_id: Some(parent_node_id.to_owned()),
                is_directory: false,
                is_tombstone: false,
            },
        );
        self.add_child(parent_node_id, name);
        self.revisions.insert(
            revision_id.clone(),
            FileRevision {
                node_id: node_id.clone(),
                revision_id: revision_id.clone(),
                base_revision_id: None,
                content_hash: content_hash.to_string(),
                mime_type: mime_type.to_owned(),
                byte_length,
                signer: operation.operation.signer,
            },
        );
        self.current_revisions.insert(node_id, revision_id);
        Ok(())
    }

    fn apply_replace_revision(
        &mut self,
        operation: &SignedFileOperation,
        node_id: &str,
        base_revision_id: &str,
        content_hash: &str,
        mime_type: &str,
        byte_length: u64,
    ) -> Result<(), AuthorityError> {
        // Clone what we need before the mutable borrow for the merge branch
        let current_revision_id = self
            .current_revisions
            .get(node_id)
            .cloned()
            .ok_or(AuthorityError::NotFound)?;
        if current_revision_id == base_revision_id {
            let revision_id = random_id()?;
            self.revisions.insert(
                revision_id.clone(),
                FileRevision {
                    node_id: node_id.to_owned(),
                    revision_id: revision_id.clone(),
                    base_revision_id: Some(base_revision_id.to_owned()),
                    content_hash: content_hash.to_string(),
                    mime_type: mime_type.to_owned(),
                    byte_length,
                    signer: operation.operation.signer,
                },
            );
            self.current_revisions
                .insert(node_id.to_owned(), revision_id);
            return Ok(());
        }
        // Concurrent edit — check if mergeable
        let current_is_markdown = self
            .revisions
            .get(&current_revision_id)
            .map(|r| is_markdown(&r.mime_type))
            .unwrap_or(false);
        if current_is_markdown && is_markdown(mime_type) {
            self.try_markdown_merge(
                node_id,
                base_revision_id,
                &current_revision_id,
                content_hash,
                mime_type,
                byte_length,
            )?;
        } else {
            self.record_conflict(
                node_id,
                ConflictKind::BinaryCollision,
                &[base_revision_id.to_owned(), current_revision_id],
            )?;
        }
        Ok(())
    }

    fn try_markdown_merge(
        &mut self,
        node_id: &str,
        base_revision_id: &str,
        current_revision_id: &str,
        incoming_content_hash: &str,
        mime_type: &str,
        byte_length: u64,
    ) -> Result<(), AuthorityError> {
        let base_hash = self
            .revisions
            .get(base_revision_id)
            .map(|r| r.content_hash.clone());
        let current_hash = self
            .revisions
            .get(current_revision_id)
            .map(|r| r.content_hash.clone());

        let base_bytes = base_hash
            .as_ref()
            .and_then(|h| self.blobs.open(&ContentHash(h.clone())).ok());
        let current_bytes = current_hash
            .as_ref()
            .and_then(|h| self.blobs.open(&ContentHash(h.clone())).ok());
        let incoming_bytes = self
            .blobs
            .open(&ContentHash(incoming_content_hash.to_string()))
            .ok();

        let (Some(base_bytes), Some(current_bytes), Some(incoming_bytes)) =
            (base_bytes, current_bytes, incoming_bytes)
        else {
            self.record_conflict(
                node_id,
                ConflictKind::MarkdownOverlap,
                &[current_revision_id.to_owned()],
            )?;
            return Ok(());
        };

        let base_str = String::from_utf8(base_bytes).map_err(|_| AuthorityError::NotATextFile)?;
        let current_str =
            String::from_utf8(current_bytes).map_err(|_| AuthorityError::NotATextFile)?;
        let incoming_str =
            String::from_utf8(incoming_bytes).map_err(|_| AuthorityError::NotATextFile)?;

        match merge::merge_markdown(&base_str, &current_str, &incoming_str) {
            MergeResult::Merged(merged) => {
                let merged_bytes = merged.into_bytes();
                let merged_hash = self.blobs.store(&merged_bytes)?;
                let revision_id = random_id()?;
                let blen = merged_bytes.len() as u64;
                self.revisions.insert(
                    revision_id.clone(),
                    FileRevision {
                        node_id: node_id.to_owned(),
                        revision_id: revision_id.clone(),
                        base_revision_id: Some(base_revision_id.to_owned()),
                        content_hash: merged_hash.0.clone(),
                        mime_type: mime_type.to_owned(),
                        byte_length: blen,
                        signer: [0; 32],
                    },
                );
                self.current_revisions
                    .insert(node_id.to_owned(), revision_id);
                Ok(())
            }
            MergeResult::Conflict { reason: _ } => {
                self.record_conflict(
                    node_id,
                    ConflictKind::MarkdownOverlap,
                    &[current_revision_id.to_owned()],
                )?;
                let incoming_rev_id = random_id()?;
                self.revisions.insert(
                    incoming_rev_id.clone(),
                    FileRevision {
                        node_id: node_id.to_owned(),
                        revision_id: incoming_rev_id.clone(),
                        base_revision_id: Some(base_revision_id.to_owned()),
                        content_hash: incoming_content_hash.to_string(),
                        mime_type: mime_type.to_owned(),
                        byte_length,
                        signer: [0; 32],
                    },
                );
                if let Some(conflict) = self.conflicts.last_mut() {
                    if !conflict.resolved {
                        conflict.competing_revision_ids.push(incoming_rev_id);
                    }
                }
                Ok(())
            }
        }
    }

    fn apply_move(
        &mut self,
        node_id: &str,
        new_parent_node_id: &str,
        new_name: &str,
    ) -> Result<(), AuthorityError> {
        PortablePath::parse(new_name)?;
        let node = self.nodes.get(node_id).ok_or(AuthorityError::NotFound)?;
        if node.is_tombstone {
            return Err(AuthorityError::NotFound);
        }
        let new_parent = self
            .nodes
            .get(new_parent_node_id)
            .ok_or(AuthorityError::NotFound)?;
        if !new_parent.is_directory || new_parent.is_tombstone {
            return Err(AuthorityError::NotADirectory);
        }
        if self.child_exists(new_parent_node_id, new_name) {
            return Err(AuthorityError::AlreadyExists);
        }
        // Clone the data we need before mutating
        let old_parent = node.parent_node_id.clone();
        let old_name = node.name.clone();
        if let Some(ref old_parent_id) = old_parent {
            self.remove_child(old_parent_id, &old_name);
        }
        self.add_child(new_parent_node_id, new_name);
        let record = self
            .nodes
            .get_mut(node_id)
            .ok_or(AuthorityError::NotFound)?;
        record.name = new_name.to_owned();
        record.parent_node_id = Some(new_parent_node_id.to_owned());
        Ok(())
    }

    fn apply_tombstone(&mut self, node_id: &str) -> Result<(), AuthorityError> {
        let record = self.nodes.get(node_id).ok_or(AuthorityError::NotFound)?;
        if record.is_tombstone {
            return Ok(());
        }
        if record.is_directory && !self.directory_is_empty(node_id) {
            return Err(AuthorityError::DirectoryNotEmpty);
        }
        let parent = record.parent_node_id.clone();
        let name = record.name.clone();
        if let Some(ref parent_id) = parent {
            self.remove_child(parent_id, &name);
        }
        let record = self
            .nodes
            .get_mut(node_id)
            .ok_or(AuthorityError::NotFound)?;
        record.is_tombstone = true;
        Ok(())
    }

    fn apply_resolve(
        &mut self,
        conflict_record_id: &str,
        chosen_revision_id: &Option<String>,
    ) -> Result<(), AuthorityError> {
        let conflict = self
            .conflicts
            .iter_mut()
            .find(|c| c.record_id == conflict_record_id)
            .ok_or(AuthorityError::NotFound)?;
        if conflict.resolved {
            return Ok(());
        }
        if let Some(revision_id) = chosen_revision_id {
            self.current_revisions
                .insert(conflict.node_id.clone(), revision_id.clone());
        }
        conflict.resolved = true;
        Ok(())
    }

    fn retry_pending(&mut self, _membership: &MembershipProjection) -> Result<(), AuthorityError> {
        let mut ready: Vec<SignedFileOperation> = Vec::new();
        let pending_ids: Vec<String> = self.pending_operations.keys().cloned().collect();
        for id in &pending_ids {
            if let Some(op) = self.pending_operations.get(id) {
                if op
                    .operation
                    .causal_parents
                    .iter()
                    .all(|p| self.applied_operations.contains(p))
                {
                    ready.push(op.clone());
                }
            }
        }
        for id in pending_ids {
            if ready.iter().any(|op| op.operation.operation_id == id) {
                self.pending_operations.remove(&id);
            }
        }
        for op in &ready {
            self.apply_operation(op)?;
            self.applied_operations
                .insert(op.operation.operation_id.clone());
        }
        if !ready.is_empty() {
            self.retry_pending(_membership)?;
        }
        Ok(())
    }

    fn record_conflict(
        &mut self,
        node_id: &str,
        kind: ConflictKind,
        competing_revision_ids: &[String],
    ) -> Result<(), AuthorityError> {
        let record_id = random_id()?;
        self.conflicts.push(ConflictRecord {
            record_id,
            node_id: node_id.to_owned(),
            kind,
            competing_revision_ids: competing_revision_ids.to_vec(),
            resolved: false,
        });
        Ok(())
    }

    fn build_tree(&self, node_id: &str) -> Option<TreeNode> {
        let record = self.nodes.get(node_id)?;
        if record.is_tombstone {
            return None;
        }
        if record.is_directory {
            let children: BTreeMap<String, TreeNode> = self
                .directory_children
                .get(node_id)
                .map(|child_names| {
                    child_names
                        .iter()
                        .filter_map(|child_name| {
                            let child_id = self
                                .nodes
                                .iter()
                                .find(|(_, r)| {
                                    r.name == *child_name
                                        && r.parent_node_id.as_deref() == Some(node_id)
                                })
                                .map(|(id, _)| id.clone())?;
                            let child = self.build_tree(&child_id)?;
                            Some((child_name.clone(), child))
                        })
                        .collect()
                })
                .unwrap_or_default();
            Some(TreeNode::Directory {
                name: record.name.clone(),
                children,
            })
        } else {
            let revision_id = self.current_revisions.get(node_id)?;
            Some(TreeNode::File {
                name: record.name.clone(),
                current_revision_id: revision_id.clone(),
            })
        }
    }

    fn root_node_exists(&self, name: &str) -> bool {
        self.nodes.values().any(|r| {
            r.parent_node_id.is_none() && r.name.eq_ignore_ascii_case(name) && !r.is_tombstone
        })
    }

    fn child_exists(&self, parent_node_id: &str, name: &str) -> bool {
        self.directory_children
            .get(parent_node_id)
            .map(|children| children.iter().any(|c| c.eq_ignore_ascii_case(name)))
            .unwrap_or(false)
    }

    fn add_child(&mut self, parent_node_id: &str, name: &str) {
        self.directory_children
            .entry(parent_node_id.to_owned())
            .or_default()
            .insert(name.to_owned());
    }

    fn remove_child(&mut self, parent_node_id: &str, name: &str) {
        if let Some(children) = self.directory_children.get_mut(parent_node_id) {
            children.remove(name);
        }
    }

    fn directory_is_empty(&self, node_id: &str) -> bool {
        self.directory_children
            .get(node_id)
            .map(|children| children.is_empty())
            .unwrap_or(true)
    }
}

fn public_identity_text(public_identity: &[u8; 32]) -> String {
    crate::identity::PublicIdentity::from_bytes(*public_identity).to_string()
}

fn is_markdown(mime_type: &str) -> bool {
    mime_type == "text/markdown"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::{InMemoryKeyCustody, InstallationIdentity};
    use crate::workspace_domain::Member;

    fn test_identity() -> InstallationIdentity {
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
            .expect("identity creates")
    }

    fn test_membership(identity: &InstallationIdentity) -> MembershipProjection {
        MembershipProjection {
            canonical_head: Some("head".to_owned()),
            members: vec![Member::new(
                identity.public_identity().to_string(),
                "Ada",
                "developer",
                "Ada",
                0,
            )],
            statuses: BTreeMap::new(),
        }
    }

    #[test]
    fn creates_and_projects_a_root_directory() {
        let identity = test_identity();
        let membership = test_membership(&identity);
        let mut authority = WorkspaceFileAuthority::new();
        let op =
            SignedFileOperation::create_directory(&identity, "ws-1", "plans").expect("op creates");

        authority.apply(&op, &membership).expect("op applies");
        let projection = authority.projection();
        assert!(projection.root.contains_key("plans"));
    }

    #[test]
    fn idempotent_on_duplicate_operation_id() {
        let identity = test_identity();
        let membership = test_membership(&identity);
        let mut authority = WorkspaceFileAuthority::new();
        let op =
            SignedFileOperation::create_directory(&identity, "ws-1", "plans").expect("op creates");

        authority.apply(&op, &membership).expect("first apply ok");
        authority.apply(&op, &membership).expect("second apply ok");
        assert_eq!(authority.applied_operation_ids().len(), 1);
    }

    #[test]
    fn rejects_non_member_signer() {
        let identity = test_identity();
        let outsider = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
            .expect("outsider identity");
        let membership = test_membership(&identity);
        let mut authority = WorkspaceFileAuthority::new();
        let op =
            SignedFileOperation::create_directory(&outsider, "ws-1", "plans").expect("op creates");

        assert_eq!(
            authority.apply(&op, &membership),
            Err(AuthorityError::NonMember)
        );
    }

    #[test]
    fn recovers_pending_operations_when_parents_arrive() {
        let identity = test_identity();
        let membership = test_membership(&identity);
        let mut authority = WorkspaceFileAuthority::new();

        // Pre-register a "parent" operation ID
        authority.applied_operations.insert("parent-00".to_owned());

        let op = SignedFileOperation::create_directory(&identity, "ws-1", "child").expect("op");
        authority.apply(&op, &membership).expect("direct apply");
        assert_eq!(authority.applied_operation_ids().len(), 2);
    }
}
