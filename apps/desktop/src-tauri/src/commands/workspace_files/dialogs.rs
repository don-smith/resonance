//! Native folder-picker and confirmation boundary for workspace files.

use tauri::AppHandle;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};

use super::command::{WorkspaceFilesError, WorkspaceFilesErrorCode};

pub(super) fn choose_confirmed_root(
    app: &AppHandle,
) -> Result<Option<std::path::PathBuf>, WorkspaceFilesError> {
    let Some(root) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let confirmed = app
        .dialog()
        .message("Use this folder only if it is new or empty and is not managed by Git.")
        .title("Confirm workspace folder")
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Use folder".to_owned(),
            "Cancel".to_owned(),
        ))
        .blocking_show();
    if !confirmed {
        return Ok(None);
    }
    root.into_path()
        .map(Some)
        .map_err(|_| WorkspaceFilesError::new(WorkspaceFilesErrorCode::UnusableRoot))
}
