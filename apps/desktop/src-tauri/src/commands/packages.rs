use resonance_runtime::packages::{PackageRegistry, PackageSource};

const BUNDLED_PACKAGE_MANIFESTS: &str =
    include_str!("../../generated/bundled-package-manifests.json");

/// Validates the generated catalog before the desktop publishes any shell.
pub fn validate_bundled_catalog() -> Result<Vec<String>, String> {
    let registry = PackageRegistry::load_catalog(PackageSource::Bundled, BUNDLED_PACKAGE_MANIFESTS)
        .map_err(|diagnostics| {
            diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.package_id, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        })?;
    Ok(registry.ids().map(str::to_owned).collect())
}
