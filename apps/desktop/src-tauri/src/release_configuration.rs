//! Desktop adapter for fork-provisioned public release configuration.

use std::{env, path::PathBuf};

use tauri::Manager;

pub use resonance_runtime::release::ReleaseConfiguration;

/// The release configuration is intentionally optional for ordinary development
/// runs. A release build bundles its CI-validated public configuration as a
/// resource; development may instead use an untracked local configuration.
pub fn load_for_current_run(app: &tauri::App) -> Result<Option<ReleaseConfiguration>, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?
        .join("release-configuration.json");
    if bundled.exists() {
        return ReleaseConfiguration::load(bundled)
            .map(Some)
            .map_err(|error| error.to_string());
    }

    let local = env::var_os("RESONANCE_RELEASE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config/release.json"));
    if !local.exists() {
        return Ok(None);
    }

    ReleaseConfiguration::load(local)
        .map(Some)
        .map_err(|error| error.to_string())
}
