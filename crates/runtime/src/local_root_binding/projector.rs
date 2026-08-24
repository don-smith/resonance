use std::{collections::BTreeMap, fs, io::Write, path::Path};

use crate::workspace_files::{
    blobs::{ContentHash, WorkspaceBlobStore},
    ignore::is_generated_conflict_path,
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
    previous: &BTreeMap<String, MaterializedRecord>,
) -> Result<BTreeMap<String, MaterializedRecord>, RootBindingError> {
    if !root.is_dir() {
        return Err(RootBindingError::Unavailable);
    }
    remove_stale_nodes(root, projection, previous)?;
    relocate_materialized_nodes(root, projection, previous)?;
    let mut records = BTreeMap::new();
    for node in projection.root.values() {
        project_node(root, "", node, projection, blobs, previous, &mut records)?;
    }
    remove_stale_materialization(root, previous, &records)?;
    Ok(records)
}

fn remove_stale_nodes(
    root: &Path,
    projection: &FileTreeProjection,
    previous: &BTreeMap<String, MaterializedRecord>,
) -> Result<(), RootBindingError> {
    let mut desired_paths = BTreeMap::new();
    for node in projection.root.values() {
        collect_projected_paths("", node, projection, &mut desired_paths);
    }
    let mut stale = previous
        .values()
        .filter(|record| !desired_paths.contains_key(&record.node_id))
        .collect::<Vec<_>>();
    stale.sort_by_key(|record| std::cmp::Reverse(record.relative_path.matches('/').count()));
    for record in stale {
        remove_materialized_record(root, record)?;
    }
    Ok(())
}

fn relocate_materialized_nodes(
    root: &Path,
    projection: &FileTreeProjection,
    previous: &BTreeMap<String, MaterializedRecord>,
) -> Result<(), RootBindingError> {
    let mut desired_paths = BTreeMap::new();
    for node in projection.root.values() {
        collect_projected_paths("", node, projection, &mut desired_paths);
    }
    for record in previous.values() {
        let Some(desired_path) = desired_paths.get(&record.node_id) else {
            continue;
        };
        if desired_path == &record.relative_path {
            continue;
        }
        let source = root.join(
            record
                .relative_path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        );
        let metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(map_projection_io(error)),
        };
        if metadata.file_type().is_symlink() {
            return Err(RootBindingError::Symlink);
        }
        if record.directory {
            if !metadata.is_dir() {
                return Err(RootBindingError::ProjectionCollision(
                    record.relative_path.clone(),
                ));
            }
        } else {
            if !metadata.is_file() {
                return Err(RootBindingError::ProjectionCollision(
                    record.relative_path.clone(),
                ));
            }
            if let Some(expected_hash) = &record.content_hash {
                let bytes = fs::read(&source).map_err(map_projection_io)?;
                if ContentHash::from_bytes(&bytes).as_str() != expected_hash {
                    return Err(RootBindingError::ProjectionCollision(
                        record.relative_path.clone(),
                    ));
                }
            }
        }
        let destination = root.join(desired_path.replace('/', std::path::MAIN_SEPARATOR_STR));
        if destination.exists() {
            return Err(RootBindingError::ProjectionCollision(desired_path.clone()));
        }
        let destination_parent = destination.parent().ok_or(RootBindingError::EscapesRoot)?;
        fs::create_dir_all(destination_parent).map_err(map_projection_io)?;
        fs::rename(&source, &destination).map_err(map_projection_io)?;
        if let Some(source_parent) = source.parent() {
            fs::File::open(source_parent)
                .and_then(|directory| directory.sync_all())
                .map_err(map_projection_io)?;
        }
        if destination.parent() != source.parent() {
            fs::File::open(destination_parent)
                .and_then(|directory| directory.sync_all())
                .map_err(map_projection_io)?;
        }
    }
    Ok(())
}

fn collect_projected_paths(
    parent: &str,
    node: &TreeNode,
    projection: &FileTreeProjection,
    paths: &mut BTreeMap<String, String>,
) {
    let relative_path = if parent.is_empty() {
        node.name().to_owned()
    } else {
        format!("{parent}/{}", node.name())
    };
    if should_ignore_projection_path(projection, &relative_path) {
        return;
    }
    paths.insert(node.node_id().to_owned(), relative_path.clone());
    if let TreeNode::Directory { children, .. } = node {
        for child in children.values() {
            collect_projected_paths(&relative_path, child, projection, paths);
        }
    }
}

fn project_node(
    root: &Path,
    parent: &str,
    node: &TreeNode,
    projection: &FileTreeProjection,
    blobs: &WorkspaceBlobStore,
    previous: &BTreeMap<String, MaterializedRecord>,
    records: &mut BTreeMap<String, MaterializedRecord>,
) -> Result<(), RootBindingError> {
    let relative_path = if parent.is_empty() {
        node.name().to_owned()
    } else {
        format!("{parent}/{}", node.name())
    };
    if should_ignore_projection_path(projection, &relative_path) {
        return Ok(());
    }
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
                project_node(
                    root,
                    &relative_path,
                    child,
                    projection,
                    blobs,
                    previous,
                    records,
                )?;
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
            let existing = fs::read(&destination).ok();
            let already_current = existing
                .as_ref()
                .is_some_and(|existing| ContentHash::from_bytes(existing) == hash);
            if !already_current {
                if let Some(existing) = existing {
                    let expected_previous = previous
                        .get(&relative_path)
                        .and_then(|record| record.content_hash.as_deref());
                    if expected_previous != Some(ContentHash::from_bytes(&existing).as_str()) {
                        return Err(RootBindingError::ProjectionCollision(relative_path));
                    }
                }
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

fn should_ignore_projection_path(projection: &FileTreeProjection, relative_path: &str) -> bool {
    !is_generated_conflict_path(relative_path)
        && projection.ignore_set.matches_configured(relative_path)
}

fn remove_stale_materialization(
    root: &Path,
    previous: &BTreeMap<String, MaterializedRecord>,
    current: &BTreeMap<String, MaterializedRecord>,
) -> Result<(), RootBindingError> {
    let mut stale = previous
        .values()
        .filter(|record| !current.contains_key(&record.relative_path))
        .collect::<Vec<_>>();
    stale.sort_by_key(|record| std::cmp::Reverse(record.relative_path.matches('/').count()));
    for record in stale {
        remove_materialized_record(root, record)?;
    }
    Ok(())
}

fn remove_materialized_record(
    root: &Path,
    record: &MaterializedRecord,
) -> Result<(), RootBindingError> {
    let path = root.join(
        record
            .relative_path
            .replace('/', std::path::MAIN_SEPARATOR_STR),
    );
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(map_projection_io(error)),
    };
    if metadata.file_type().is_symlink() {
        return Err(RootBindingError::Symlink);
    }
    if record.directory {
        if !metadata.is_dir() {
            return Err(RootBindingError::ProjectionCollision(
                record.relative_path.clone(),
            ));
        }
        fs::remove_dir(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::DirectoryNotEmpty {
                RootBindingError::ProjectionCollision(record.relative_path.clone())
            } else {
                map_projection_io(error)
            }
        })?;
    } else {
        if !metadata.is_file() {
            return Err(RootBindingError::ProjectionCollision(
                record.relative_path.clone(),
            ));
        }
        if let Some(expected_hash) = &record.content_hash {
            let bytes = fs::read(&path).map_err(map_projection_io)?;
            if ContentHash::from_bytes(&bytes).as_str() != expected_hash {
                return Err(RootBindingError::ProjectionCollision(
                    record.relative_path.clone(),
                ));
            }
        }
        fs::remove_file(&path).map_err(map_projection_io)?;
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
