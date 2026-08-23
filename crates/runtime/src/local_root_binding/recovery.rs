use std::{fs, path::Path};

use super::RootBindingError;

pub(crate) fn remove_interrupted_writes(root: &Path) -> Result<(), RootBindingError> {
    if !root.is_dir() {
        return Err(RootBindingError::Unavailable);
    }
    visit(root)
}

fn visit(directory: &Path) -> Result<(), RootBindingError> {
    for entry in fs::read_dir(directory).map_err(|error| RootBindingError::Io(error.to_string()))? {
        let entry = entry.map_err(|error| RootBindingError::Io(error.to_string()))?;
        let path = entry.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| RootBindingError::Io(error.to_string()))?;
        if metadata.file_type().is_symlink() {
            return Err(RootBindingError::Symlink);
        }
        if metadata.is_dir() {
            visit(&path)?;
        } else if entry
            .file_name()
            .to_string_lossy()
            .starts_with(".resonance-write-")
        {
            fs::remove_file(path).map_err(|error| RootBindingError::Io(error.to_string()))?;
        }
    }
    Ok(())
}
