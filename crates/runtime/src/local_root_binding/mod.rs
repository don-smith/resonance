//! Private local-root binding, projection, polling, and recovery.

mod ingestor;
mod projector;
mod recovery;
mod watcher;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    identity::InstallationIdentity,
    workspace_files::{
        blobs::WorkspaceBlobStore,
        paths::{PathError, PortablePath},
        projection::FileTreeProjection,
        FileOperationError, SignedFileOperation,
    },
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

use self::ingestor::FilesystemIngestor;
pub(crate) use self::projector::MaterializedRecord;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootSelection {
    ConfirmedNotGitManaged,
    NotConfirmed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootHealth {
    Healthy,
    Unavailable,
    Unwritable,
    Unhealthy,
}

impl RootHealth {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Unavailable => "unavailable",
            Self::Unwritable => "unwritable",
            Self::Unhealthy => "unhealthy",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "healthy" => Some(Self::Healthy),
            "unavailable" => Some(Self::Unavailable),
            "unwritable" => Some(Self::Unwritable),
            "unhealthy" => Some(Self::Unhealthy),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LocalChange {
    CreateDirectory {
        relative_path: String,
    },
    CreateFile {
        relative_path: String,
        content_hash: String,
        mime_type: String,
        bytes: Vec<u8>,
    },
    ReplaceFile {
        node_id: String,
        base_revision_id: String,
        relative_path: String,
        content_hash: String,
        bytes: Vec<u8>,
    },
    MoveNode {
        node_id: String,
        new_relative_path: String,
    },
    TombstoneNode {
        node_id: String,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub enum RootBindingError {
    GitStatusUnconfirmed,
    NotAbsolute,
    NotDirectory,
    NonEmpty,
    GitMetadata,
    Symlink,
    SpecialFile,
    NonUnicodePath,
    CaseFoldCollision,
    UnsafePath(String),
    EscapesRoot,
    Unavailable,
    Unwritable,
    Unreadable,
    ProjectionCollision(String),
    MissingRevision(String),
    MissingBlob(String),
    InvalidBlob(String),
    MissingParent(String),
    Signing(String),
    Storage(String),
    Io(String),
}

impl std::fmt::Display for RootBindingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GitStatusUnconfirmed => {
                formatter.write_str("root was not confirmed outside Git management")
            }
            Self::NotAbsolute => formatter.write_str("local root path must be absolute"),
            Self::NotDirectory => formatter.write_str("local root is not a directory"),
            Self::NonEmpty => formatter.write_str("local root must be empty when first bound"),
            Self::GitMetadata => formatter.write_str("local root contains Git metadata"),
            Self::Symlink => {
                formatter.write_str("local root contains or resolves through a symlink")
            }
            Self::SpecialFile => formatter.write_str("local root contains a special file"),
            Self::NonUnicodePath => formatter.write_str("local root contains a non-Unicode path"),
            Self::CaseFoldCollision => {
                formatter.write_str("local root contains case-fold colliding names")
            }
            Self::UnsafePath(reason) => {
                write!(formatter, "local root contains an unsafe path: {reason}")
            }
            Self::EscapesRoot => formatter.write_str("local path escapes the bound root"),
            Self::Unavailable => formatter.write_str("local root is unavailable"),
            Self::Unwritable => formatter.write_str("local root is not writable"),
            Self::Unreadable => formatter.write_str("local root contains an unreadable file"),
            Self::ProjectionCollision(path) => write!(formatter, "projection collides at {path}"),
            Self::MissingRevision(id) => write!(formatter, "projection revision {id} is missing"),
            Self::MissingBlob(hash) => write!(formatter, "projection blob {hash} is missing"),
            Self::InvalidBlob(hash) => {
                write!(formatter, "projection blob {hash} failed verification")
            }
            Self::MissingParent(path) => {
                write!(formatter, "local change parent {path} is not materialized")
            }
            Self::Signing(reason) => {
                write!(formatter, "local change could not be signed: {reason}")
            }
            Self::Storage(reason) => {
                write!(formatter, "local-root state could not be stored: {reason}")
            }
            Self::Io(reason) => write!(formatter, "local-root I/O failed: {reason}"),
        }
    }
}

impl std::error::Error for RootBindingError {}

impl From<PathError> for RootBindingError {
    fn from(error: PathError) -> Self {
        Self::UnsafePath(error.to_string())
    }
}

impl From<FileOperationError> for RootBindingError {
    fn from(error: FileOperationError) -> Self {
        Self::Signing(error.to_string())
    }
}

impl From<crate::workspace_files::blobs::BlobError> for RootBindingError {
    fn from(error: crate::workspace_files::blobs::BlobError) -> Self {
        Self::InvalidBlob(error.to_string())
    }
}

impl From<WorkspaceStoreError> for RootBindingError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Storage(error.to_string())
    }
}

#[derive(Debug)]
pub struct LocalRootBinding<'a> {
    store: &'a WorkspaceStore,
    root: PathBuf,
    health: RootHealth,
    materialized: BTreeMap<String, MaterializedRecord>,
    ingestor: FilesystemIngestor,
}

impl<'a> LocalRootBinding<'a> {
    pub fn bind(
        store: &'a WorkspaceStore,
        root: impl AsRef<Path>,
        selection: RootSelection,
        projection: &FileTreeProjection,
        blobs: &WorkspaceBlobStore,
    ) -> Result<Self, RootBindingError> {
        if selection != RootSelection::ConfirmedNotGitManaged {
            return Err(RootBindingError::GitStatusUnconfirmed);
        }
        let root = root.as_ref().to_path_buf();
        if !root.is_absolute() {
            return Err(RootBindingError::NotAbsolute);
        }
        prepare_empty_root(&root)?;
        let materialized = projector::project(&root, projection, blobs)?;
        let snapshot = watcher::snapshot(&root)?;
        store.set_local_root_binding(&root, RootHealth::Healthy)?;
        store.replace_local_root_materialization(materialized.values())?;
        Ok(Self {
            store,
            root,
            health: RootHealth::Healthy,
            materialized,
            ingestor: FilesystemIngestor::new(snapshot),
        })
    }

    #[must_use]
    pub const fn health(&self) -> RootHealth {
        self.health
    }

    pub fn poll_changes(&mut self) -> Result<Vec<LocalChange>, RootBindingError> {
        match watcher::snapshot(&self.root) {
            Ok(snapshot) => {
                self.set_health(RootHealth::Healthy, None)?;
                let changes = self.ingestor.observe(snapshot, &mut self.materialized);
                self.store
                    .replace_local_root_materialization(self.materialized.values())?;
                Ok(changes)
            }
            Err(error) => {
                let health = match error {
                    RootBindingError::Unavailable => RootHealth::Unavailable,
                    RootBindingError::Unwritable => RootHealth::Unwritable,
                    _ => RootHealth::Unhealthy,
                };
                self.set_health(health, Some(error.to_string()))?;
                Err(error)
            }
        }
    }

    pub fn author_changes(
        &mut self,
        identity: &InstallationIdentity,
        workspace_id: &str,
        changes: &[LocalChange],
        blobs: &mut WorkspaceBlobStore,
        mut causal_frontier: Vec<String>,
    ) -> Result<Vec<SignedFileOperation>, RootBindingError> {
        let mut operations = Vec::new();
        for change in changes {
            let operation = match change {
                LocalChange::CreateDirectory { relative_path } => {
                    let (parent_node_id, name) = self.parent_and_name(relative_path)?;
                    let operation = SignedFileOperation::create_directory(
                        identity,
                        workspace_id,
                        parent_node_id,
                        name,
                        causal_frontier,
                    )?;
                    self.materialized.insert(
                        relative_path.clone(),
                        MaterializedRecord {
                            node_id: operation.operation.node_id.clone(),
                            relative_path: relative_path.clone(),
                            revision_id: None,
                            content_hash: None,
                            directory: true,
                        },
                    );
                    operation
                }
                LocalChange::CreateFile {
                    relative_path,
                    content_hash,
                    mime_type,
                    bytes,
                } => {
                    let (parent_node_id, name) = self.parent_and_name(relative_path)?;
                    let hash = blobs.store(bytes)?;
                    if hash.as_str() != content_hash {
                        return Err(RootBindingError::InvalidBlob(content_hash.clone()));
                    }
                    let operation = SignedFileOperation::create_file(
                        identity,
                        workspace_id,
                        parent_node_id,
                        name,
                        content_hash,
                        mime_type,
                        bytes.len() as u64,
                        causal_frontier,
                    )?;
                    self.materialized.insert(
                        relative_path.clone(),
                        MaterializedRecord {
                            node_id: operation.operation.node_id.clone(),
                            relative_path: relative_path.clone(),
                            revision_id: Some(operation.operation.operation_id.clone()),
                            content_hash: Some(content_hash.clone()),
                            directory: false,
                        },
                    );
                    operation
                }
                LocalChange::ReplaceFile {
                    node_id,
                    base_revision_id,
                    content_hash,
                    bytes,
                    ..
                } => {
                    let hash = blobs.store(bytes)?;
                    if hash.as_str() != content_hash {
                        return Err(RootBindingError::InvalidBlob(content_hash.clone()));
                    }
                    SignedFileOperation::replace_file_revision(
                        identity,
                        workspace_id,
                        node_id,
                        base_revision_id,
                        content_hash,
                        "text/markdown",
                        bytes.len() as u64,
                        causal_frontier,
                    )?
                }
                LocalChange::MoveNode {
                    node_id,
                    new_relative_path,
                } => {
                    let (parent_node_id, name) = self.parent_and_name(new_relative_path)?;
                    SignedFileOperation::move_node_to(
                        identity,
                        workspace_id,
                        node_id,
                        parent_node_id,
                        name,
                        causal_frontier,
                    )?
                }
                LocalChange::TombstoneNode { node_id } => SignedFileOperation::tombstone_node(
                    identity,
                    workspace_id,
                    node_id,
                    causal_frontier,
                )?,
            };
            causal_frontier = vec![operation.operation.operation_id.clone()];
            operations.push(operation);
        }
        self.store
            .replace_local_root_materialization(self.materialized.values())?;
        Ok(operations)
    }

    pub fn replace_root(
        &mut self,
        root: impl AsRef<Path>,
        selection: RootSelection,
        projection: &FileTreeProjection,
        blobs: &WorkspaceBlobStore,
    ) -> Result<(), RootBindingError> {
        if selection != RootSelection::ConfirmedNotGitManaged {
            return Err(RootBindingError::GitStatusUnconfirmed);
        }
        let root = root.as_ref().to_path_buf();
        if !root.is_absolute() {
            return Err(RootBindingError::NotAbsolute);
        }
        prepare_empty_root(&root)?;
        let materialized = projector::project(&root, projection, blobs)?;
        let snapshot = watcher::snapshot(&root)?;
        self.store
            .set_local_root_binding(&root, RootHealth::Healthy)?;
        self.store
            .replace_local_root_materialization(materialized.values())?;
        self.root = root;
        self.materialized = materialized;
        self.ingestor = FilesystemIngestor::new(snapshot);
        self.health = RootHealth::Healthy;
        Ok(())
    }

    pub fn repair(
        &mut self,
        projection: &FileTreeProjection,
        blobs: &WorkspaceBlobStore,
    ) -> Result<(), RootBindingError> {
        recovery::remove_interrupted_writes(&self.root)?;
        self.materialized = projector::project(&self.root, projection, blobs)?;
        let snapshot = watcher::snapshot(&self.root)?;
        self.ingestor = FilesystemIngestor::new(snapshot);
        self.store
            .replace_local_root_materialization(self.materialized.values())?;
        self.set_health(RootHealth::Healthy, None)
    }

    fn parent_and_name(
        &self,
        relative_path: &str,
    ) -> Result<(Option<String>, String), RootBindingError> {
        let path = PortablePath::parse(relative_path)?;
        let name = path
            .name()
            .ok_or_else(|| RootBindingError::UnsafePath("path has no name".to_owned()))?
            .to_owned();
        let parent_path = path.parent_segments().join("/");
        if parent_path.is_empty() {
            return Ok((None, name));
        }
        let parent = self
            .materialized
            .get(&parent_path)
            .filter(|record| record.directory)
            .ok_or_else(|| RootBindingError::MissingParent(parent_path.clone()))?;
        Ok((Some(parent.node_id.clone()), name))
    }

    fn set_health(
        &mut self,
        health: RootHealth,
        error: Option<String>,
    ) -> Result<(), RootBindingError> {
        self.health = health;
        self.store
            .update_local_root_health(health, error.as_deref())?;
        Ok(())
    }
}

fn prepare_empty_root(root: &Path) -> Result<(), RootBindingError> {
    match fs::symlink_metadata(root) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() {
                return Err(RootBindingError::Symlink);
            }
            if !metadata.is_dir() {
                return Err(RootBindingError::NotDirectory);
            }
            let snapshot = watcher::snapshot(root)?;
            if !snapshot.is_empty() {
                return Err(RootBindingError::NonEmpty);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(root).map_err(|error| RootBindingError::Io(error.to_string()))?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return Err(RootBindingError::Unwritable);
        }
        Err(error) => return Err(RootBindingError::Io(error.to_string())),
    }
    Ok(())
}
