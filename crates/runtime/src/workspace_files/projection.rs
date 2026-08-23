//! Deterministic logical-tree projection from signed file operations.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeNode {
    Directory {
        node_id: String,
        name: String,
        children: BTreeMap<String, TreeNode>,
    },
    File {
        node_id: String,
        name: String,
        current_revision_id: String,
    },
}

impl TreeNode {
    #[must_use]
    pub fn node_id(&self) -> &str {
        match self {
            Self::Directory { node_id, .. } | Self::File { node_id, .. } => node_id,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Directory { name, .. } | Self::File { name, .. } => name,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileRevision {
    pub node_id: String,
    pub revision_id: String,
    pub base_revision_id: Option<String>,
    pub content_hash: String,
    pub mime_type: String,
    pub byte_length: u64,
    pub signer: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConflictRecord {
    pub record_id: String,
    pub node_id: String,
    pub kind: ConflictKind,
    pub competing_revision_ids: Vec<String>,
    pub resolved: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConflictKind {
    MarkdownOverlap,
    BinaryCollision,
    DeleteEdit,
    ConcurrentCreate,
    CompetingMove,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileTreeProjection {
    pub root: BTreeMap<String, TreeNode>,
    pub revisions: BTreeMap<String, FileRevision>,
    pub conflicts: Vec<ConflictRecord>,
}
