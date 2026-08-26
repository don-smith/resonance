//! Finite workspace-files domain-to-wire error mapping.

use std::collections::BTreeSet;

use resonance_runtime::workspace_file_runtime::WorkspaceFileRuntimeError;

use super::command::{WorkspaceFilesError, WorkspaceFilesErrorCode};

pub(super) fn unavailable() -> WorkspaceFilesError {
    WorkspaceFilesError::new(WorkspaceFilesErrorCode::UnavailableCapability)
}

pub(super) fn revision_error(error: WorkspaceFileRuntimeError) -> WorkspaceFilesError {
    match error {
        WorkspaceFileRuntimeError::NotFound | WorkspaceFileRuntimeError::NotMarkdown => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::MissingRevision)
        }
        WorkspaceFileRuntimeError::TooLarge => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::SizeLimit)
        }
        _ => WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal),
    }
}

pub(super) fn mutation_error(error: WorkspaceFileRuntimeError) -> WorkspaceFilesError {
    match error {
        WorkspaceFileRuntimeError::NotFound | WorkspaceFileRuntimeError::NotMarkdown => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::MissingRevision)
        }
        WorkspaceFileRuntimeError::StaleRevision => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::StaleRevision)
        }
        WorkspaceFileRuntimeError::InvalidName => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::InvalidMarkdownName)
        }
        WorkspaceFileRuntimeError::TooLarge => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::SizeLimit)
        }
        _ => WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal),
    }
}

pub(super) fn root_error(error: WorkspaceFileRuntimeError) -> WorkspaceFilesError {
    match error {
        WorkspaceFileRuntimeError::Root(_) => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::UnusableRoot)
        }
        _ => WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal),
    }
}

pub(super) fn conflict_error(error: WorkspaceFileRuntimeError) -> WorkspaceFilesError {
    match error {
        WorkspaceFileRuntimeError::ChangedConflictChoice => {
            WorkspaceFilesError::new(WorkspaceFilesErrorCode::ChangedConflictChoice)
        }
        _ => WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal),
    }
}

pub(super) fn valid_identifier(value: &str) -> bool {
    !value.is_empty() && value.chars().count() <= super::command::MAX_IDENTIFIER_LENGTH
}

pub(super) fn valid_identifiers(values: &[String]) -> bool {
    values.len() <= super::command::MAX_CONFLICT_ITEMS
        && values.iter().all(|value| valid_identifier(value))
        && values.iter().collect::<BTreeSet<_>>().len() == values.len()
}

#[cfg(test)]
mod tests {
    use resonance_runtime::workspace_file_runtime::WorkspaceFileRuntimeError;

    use super::{conflict_error, root_error};
    use crate::commands::workspace_files::command::{WorkspaceFilesError, WorkspaceFilesErrorCode};

    #[test]
    fn preserves_the_difference_between_stale_choices_and_internal_failures() {
        assert_eq!(
            conflict_error(WorkspaceFileRuntimeError::ChangedConflictChoice).code,
            WorkspaceFilesErrorCode::ChangedConflictChoice
        );
        assert_eq!(
            conflict_error(WorkspaceFileRuntimeError::NotFound).code,
            WorkspaceFilesErrorCode::Internal
        );
        assert_eq!(
            root_error(WorkspaceFileRuntimeError::NotFound).code,
            WorkspaceFilesErrorCode::Internal
        );
        let _ = WorkspaceFilesErrorCode::Internal;
        let _ = WorkspaceFilesError::new(WorkspaceFilesErrorCode::Internal);
    }
}
