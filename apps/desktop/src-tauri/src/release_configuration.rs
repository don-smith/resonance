//! Desktop adapter for fork-provisioned public release configuration.

use std::{env, path::PathBuf};

use tauri::Manager;

pub use resonance_runtime::release::ReleaseConfiguration;

/// The release configuration is intentionally optional for ordinary development
/// runs. Release builds use their CI-validated bundled resource. Debug builds
/// ignore that resource because a stale file in `target/debug` must not make an
/// otherwise unconfigured development run fail.
pub fn load_for_current_run(app: &tauri::App) -> Result<Option<ReleaseConfiguration>, String> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?
        .join("release-configuration.json");
    let local = env::var_os("RESONANCE_RELEASE_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config/release.json"));
    load_from_paths(bundled, local, !cfg!(debug_assertions))
}

fn load_from_paths(
    bundled: PathBuf,
    local: PathBuf,
    use_bundled: bool,
) -> Result<Option<ReleaseConfiguration>, String> {
    if use_bundled && bundled.exists() {
        return ReleaseConfiguration::load(bundled)
            .map(Some)
            .map_err(|error| error.to_string());
    }
    if !local.exists() {
        return Ok(None);
    }
    ReleaseConfiguration::load(local)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::load_from_paths;

    #[test]
    fn debug_runs_ignore_a_stale_bundled_release_configuration() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time must be after Unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("resonance-release-{nonce}"));
        fs::create_dir_all(&directory).expect("temporary directory creates");
        let bundled = directory.join("release-configuration.json");
        fs::write(
            &bundled,
            r#"{"manifestEndpoint":"https://REPLACE_WITH_UPDATE_HOST/latest.json","artifactBaseUrl":"https://REPLACE_WITH_DOWNLOAD_HOST/releases/v0.1.0","publicKey":"REPLACE_WITH_A_PUBLIC_KEY","targets":{}}"#,
        )
        .expect("placeholder configuration writes");

        assert!(
            load_from_paths(bundled.clone(), directory.join("missing.json"), false)
                .expect("debug run ignores the bundled resource")
                .is_none()
        );
        assert!(load_from_paths(bundled, directory.join("missing.json"), true).is_err());

        fs::remove_dir_all(directory).expect("temporary directory cleans up");
    }
}
