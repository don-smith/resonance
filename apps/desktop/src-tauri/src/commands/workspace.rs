use std::path::Path;

use resonance_runtime::workspace_store::WorkspaceStore;

const DEFAULT_WORKSPACE_ID: &str = "default";

/// Opens the one development workspace without exposing application paths to
/// frontend callers. Workspace selection is intentionally deferred.
pub fn bootstrap_default_workspace(application_data: &Path) -> Result<(), String> {
    WorkspaceStore::open(application_data, DEFAULT_WORKSPACE_ID)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn default_workspace_status() -> &'static str {
    "Local workspace ready"
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::bootstrap_default_workspace;

    #[test]
    fn bootstrap_opens_one_opaque_default_workspace() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after Unix epoch")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("resonance-desktop-{nonce}"));

        bootstrap_default_workspace(&root).expect("default workspace opens");

        assert!(root
            .join(".resonance/workspaces/default/workspace.sqlite3")
            .is_file());
        fs::remove_dir_all(root).expect("temporary directory cleans up");
    }
}
