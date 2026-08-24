use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
    sync::Arc,
};

use crate::workspace_files::{
    blobs::ContentHash,
    ignore::{is_generated_conflict_path, is_git_path, WorkspaceIgnoreSet},
    paths::PortablePath,
};

use super::RootBindingError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SnapshotEntry {
    Directory,
    File { hash: String, bytes: Arc<[u8]> },
}

pub(crate) type RootSnapshot = BTreeMap<String, SnapshotEntry>;

pub(crate) fn snapshot(
    root: &Path,
    ignore_set: &WorkspaceIgnoreSet,
) -> Result<RootSnapshot, RootBindingError> {
    snapshot_with_policy(root, ignore_set, false)
}

pub(crate) fn snapshot_for_binding(root: &Path) -> Result<RootSnapshot, RootBindingError> {
    snapshot_with_policy(root, &WorkspaceIgnoreSet::default(), true)
}

fn snapshot_with_policy(
    root: &Path,
    ignore_set: &WorkspaceIgnoreSet,
    reject_git: bool,
) -> Result<RootSnapshot, RootBindingError> {
    if !root.exists() {
        return Err(RootBindingError::Unavailable);
    }
    let metadata = fs::symlink_metadata(root).map_err(map_root_io)?;
    if metadata.file_type().is_symlink() {
        return Err(RootBindingError::Symlink);
    }
    if !metadata.is_dir() {
        return Err(RootBindingError::NotDirectory);
    }
    let mut snapshot = BTreeMap::new();
    visit(root, root, ignore_set, reject_git, &mut snapshot)?;
    Ok(snapshot)
}

fn visit(
    root: &Path,
    directory: &Path,
    ignore_set: &WorkspaceIgnoreSet,
    reject_git: bool,
    snapshot: &mut RootSnapshot,
) -> Result<(), RootBindingError> {
    let mut folded_names = BTreeSet::new();
    for entry in fs::read_dir(directory).map_err(map_root_io)? {
        let entry = entry.map_err(|_| RootBindingError::Unreadable)?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|_| RootBindingError::Unreadable)?;
        if metadata.file_type().is_symlink() {
            return Err(RootBindingError::Symlink);
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|_| RootBindingError::EscapesRoot)?;
        let relative = relative
            .to_str()
            .ok_or(RootBindingError::NonUnicodePath)?
            .replace(std::path::MAIN_SEPARATOR, "/");
        if is_git_path(&relative) {
            if reject_git {
                return Err(RootBindingError::GitMetadata);
            }
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if (!reject_git && name.starts_with(".resonance-write-"))
            || (!reject_git && is_generated_conflict_path(&relative))
            || (!reject_git && ignore_set.matches_configured(&relative))
        {
            continue;
        }
        let folded = name.to_lowercase();
        if !folded_names.insert(folded) {
            return Err(RootBindingError::CaseFoldCollision);
        }
        PortablePath::parse(&relative)?;
        if metadata.is_dir() {
            snapshot.insert(relative.clone(), SnapshotEntry::Directory);
            visit(root, &path, ignore_set, reject_git, snapshot)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path).map_err(|_| RootBindingError::Unreadable)?;
            snapshot.insert(
                relative,
                SnapshotEntry::File {
                    hash: ContentHash::from_bytes(&bytes).as_str().to_owned(),
                    bytes: bytes.into(),
                },
            );
        } else {
            return Err(RootBindingError::SpecialFile);
        }
    }
    Ok(())
}

fn map_root_io(error: std::io::Error) -> RootBindingError {
    match error.kind() {
        std::io::ErrorKind::NotFound => RootBindingError::Unavailable,
        std::io::ErrorKind::PermissionDenied => RootBindingError::Unwritable,
        _ => RootBindingError::Io(error.to_string()),
    }
}
