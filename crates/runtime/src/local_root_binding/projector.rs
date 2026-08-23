use std::{collections::BTreeMap, fs, io::Write, path::Path};

use crate::workspace_files::{
    blobs::{ContentHash, WorkspaceBlobStore},
    projection::{FileTreeProjection, TreeNode},
};

use super::RootBindingError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MaterializedRecord {
    pub node_id: String,
    pub relative_path: String,
    pub revision_id: Option<String>,
    pub content_hash: Option<String>,
    pub directory: bool,
}

pub(crate) fn project(
    root: &Path,
    projection: &FileTreeProjection,
    blobs: &WorkspaceBlobStore,
) -> Result<BTreeMap<String, MaterializedRecord>, RootBindingError> {
    if !root.is_dir() {
        return Err(RootBindingError::Unavailable);
    }
    let mut records = BTreeMap::new();
    for node in projection.root.values() {
        project_node(root, "", node, projection, blobs, &mut records)?;
    }
    Ok(records)
}

fn project_node(
    root: &Path,
    parent: &str,
    node: &TreeNode,
    projection: &FileTreeProjection,
    blobs: &WorkspaceBlobStore,
    records: &mut BTreeMap<String, MaterializedRecord>,
) -> Result<(), RootBindingError> {
    let relative_path = if parent.is_empty() {
        node.name().to_owned()
    } else {
        format!("{parent}/{}", node.name())
    };
    let destination = root.join(relative_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    match node {
        TreeNode::Directory {
            node_id, children, ..
        } => {
            if destination.exists() && !destination.is_dir() {
                return Err(RootBindingError::ProjectionCollision(relative_path));
            }
            fs::create_dir_all(&destination).map_err(map_projection_io)?;
            records.insert(
                relative_path.clone(),
                MaterializedRecord {
                    node_id: node_id.clone(),
                    relative_path: relative_path.clone(),
                    revision_id: None,
                    content_hash: None,
                    directory: true,
                },
            );
            for child in children.values() {
                project_node(root, &relative_path, child, projection, blobs, records)?;
            }
        }
        TreeNode::File {
            node_id,
            current_revision_id,
            ..
        } => {
            let revision = projection
                .revisions
                .get(current_revision_id)
                .ok_or_else(|| RootBindingError::MissingRevision(current_revision_id.clone()))?;
            let hash = ContentHash(revision.content_hash.clone());
            let bytes = blobs
                .open(&hash)
                .map_err(|_| RootBindingError::MissingBlob(hash.0.clone()))?;
            if bytes.len() as u64 != revision.byte_length || ContentHash::from_bytes(&bytes) != hash
            {
                return Err(RootBindingError::InvalidBlob(hash.0));
            }
            if destination.exists() && destination.is_dir() {
                return Err(RootBindingError::ProjectionCollision(relative_path));
            }
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(map_projection_io)?;
            }
            let already_current = fs::read(&destination)
                .ok()
                .is_some_and(|existing| ContentHash::from_bytes(&existing) == hash);
            if !already_current {
                atomic_write(&destination, &bytes, hash.as_str())?;
            }
            records.insert(
                relative_path.clone(),
                MaterializedRecord {
                    node_id: node_id.clone(),
                    relative_path,
                    revision_id: Some(current_revision_id.clone()),
                    content_hash: Some(hash.0),
                    directory: false,
                },
            );
        }
    }
    Ok(())
}

fn atomic_write(destination: &Path, bytes: &[u8], hash: &str) -> Result<(), RootBindingError> {
    let parent = destination.parent().ok_or(RootBindingError::EscapesRoot)?;
    let temporary = parent.join(format!(
        ".resonance-write-{}-{}.tmp",
        std::process::id(),
        &hash[..12]
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, destination)?;
        fs::File::open(parent)?.sync_all()?;
        Ok::<(), std::io::Error>(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(map_projection_io(error));
    }
    Ok(())
}

fn map_projection_io(error: std::io::Error) -> RootBindingError {
    match error.kind() {
        std::io::ErrorKind::NotFound => RootBindingError::Unavailable,
        std::io::ErrorKind::PermissionDenied => RootBindingError::Unwritable,
        _ => RootBindingError::Io(error.to_string()),
    }
}
