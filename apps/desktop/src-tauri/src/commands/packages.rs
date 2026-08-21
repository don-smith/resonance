use resonance_runtime::packages::{PackageRegistry, PackageSource};

const REFERENCE_MANIFEST: &str =
    include_str!("../../../../../packages/reference-package/manifest.json");

/// Narrow desktop adapter: package discovery stays inside the runtime registry.
#[tauri::command]
pub fn bundled_package_ids() -> Result<Vec<String>, String> {
    let registry = PackageRegistry::load(PackageSource::BundledTeam, &[REFERENCE_MANIFEST])
        .map_err(|diagnostics| {
            diagnostics
                .into_iter()
                .map(|diagnostic| format!("{}: {}", diagnostic.package_id, diagnostic.message))
                .collect::<Vec<_>>()
                .join("; ")
        })?;

    Ok(["resonance.reference"]
        .into_iter()
        .filter(|id| registry.get(id).is_some())
        .map(str::to_owned)
        .collect())
}
