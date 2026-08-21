//! Validated public configuration for the signed-update delivery seam.
//!
//! The private signing key is intentionally not represented here: it belongs
//! exclusively in CI. This module validates only the public, fork-owned data
//! that a desktop build may use to locate and verify an update manifest.

use std::{collections::BTreeMap, fmt, fs, path::Path};

use serde::Deserialize;
use url::Url;

pub const REQUIRED_TARGETS: [&str; 3] = ["darwin-aarch64", "darwin-x86_64", "windows-x86_64"];

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseConfiguration {
    pub manifest_endpoint: Url,
    pub artifact_base_url: Url,
    pub public_key: String,
    pub targets: BTreeMap<String, ReleaseTarget>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReleaseTarget {
    pub artifact: String,
}

#[derive(Debug)]
pub struct ReleaseConfigurationError(String);

impl fmt::Display for ReleaseConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for ReleaseConfigurationError {}

impl ReleaseConfiguration {
    /// Loads a provisioned, public release configuration from JSON.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ReleaseConfigurationError> {
        let source = fs::read_to_string(path).map_err(|error| {
            ReleaseConfigurationError(format!("could not read release configuration: {error}"))
        })?;
        let configuration: Self = serde_json::from_str(&source).map_err(|error| {
            ReleaseConfigurationError(format!("release configuration is malformed: {error}"))
        })?;
        configuration.validate()?;
        Ok(configuration)
    }

    /// Rejects values that could accidentally make a release appear configured.
    pub fn validate(&self) -> Result<(), ReleaseConfigurationError> {
        if self.manifest_endpoint.scheme() != "https" || self.manifest_endpoint.host().is_none() {
            return Err(ReleaseConfigurationError(
                "manifestEndpoint must be an absolute HTTPS URL".into(),
            ));
        }
        if self.artifact_base_url.scheme() != "https" || self.artifact_base_url.host().is_none() {
            return Err(ReleaseConfigurationError(
                "artifactBaseUrl must be an absolute HTTPS URL".into(),
            ));
        }
        if is_placeholder(&self.public_key) {
            return Err(ReleaseConfigurationError(
                "publicKey must be a provisioned, non-placeholder updater key".into(),
            ));
        }
        for target in REQUIRED_TARGETS {
            let Some(metadata) = self.targets.get(target) else {
                return Err(ReleaseConfigurationError(format!(
                    "targets must include {target}"
                )));
            };
            if metadata.artifact.trim().is_empty() || metadata.artifact.contains("..") {
                return Err(ReleaseConfigurationError(format!(
                    "targets.{target}.artifact must be a non-empty artifact filename"
                )));
            }
        }
        Ok(())
    }
}

fn is_placeholder(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.len() < 32
        || [
            "placeholder",
            "replace",
            "example",
            "your-",
            "changeme",
            "<",
        ]
        .iter()
        .any(|marker| normalized.contains(marker))
}
