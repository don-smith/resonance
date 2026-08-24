use resonance_runtime::packages::{PackageRegistry, PackageSource};

const BUNDLED_PACKAGE_MANIFESTS: &str =
    include_str!("../../generated/bundled-package-manifests.json");

/// Returns package IDs from the generated, validated bundled catalog.
#[tauri::command]
pub fn bundled_package_ids() -> Result<Vec<String>, String> {
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
