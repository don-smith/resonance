//! Deterministic tree projection from signed file operations.

use std::collections::BTreeMap;

/// A node in the projected file tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeNode {
    Directory {
        name: String,
        children: BTreeMap<String, TreeNode>,
    },
    File {
        name: String,
        current_revision_id: String,
    },
    Tombstone {
        name: String,
    },
}

impl TreeNode {
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Directory { name, .. } | Self::File { name, .. } | Self::Tombstone { name } => {
                name
            }
        }
    }
}

/// A projected file-tree revision.
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

/// A conflict record produced when concurrent operations cannot merge safely.
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

/// The current projection state.
#[derive(Clone, Debug, Default)]
pub struct FileTreeProjection {
    pub root: BTreeMap<String, TreeNode>,
    pub revisions: BTreeMap<String, FileRevision>,
    pub conflicts: Vec<ConflictRecord>,
}

impl FileTreeProjection {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
