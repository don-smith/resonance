//! The sole desktop adapter allowed to construct a Tauri updater.
//!
//! Nothing in the package runtime receives this service or updater capability.
//! A configured service constructs a verifier from the validated public release
//! configuration; it does not contact an endpoint until a future shell-owned
//! update interaction explicitly calls `check`.

use resonance_runtime::release::ReleaseConfiguration;
use tauri_plugin_updater::{Updater, UpdaterExt};

pub struct UpdateService {
    configuration: ReleaseConfiguration,
}

impl UpdateService {
    pub const fn new(configuration: ReleaseConfiguration) -> Self {
        Self { configuration }
    }

    pub fn updater(&self, app: &tauri::AppHandle) -> Result<Updater, String> {
        app.updater_builder()
            .pubkey(self.configuration.public_key.clone())
            .endpoints(vec![self.configuration.manifest_endpoint.clone()])
            .map_err(|error| error.to_string())?
            .build()
            .map_err(|error| error.to_string())
    }
}
