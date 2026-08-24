const APPLICATION_COMMANDS: &[&str] = &[
    "bundled_package_ids",
    "workspace_view",
    "workspace_files_v1",
    "create_workspace",
    "create_workspace_invite",
    "join_workspace",
    "retry_workspace_join",
];

fn main() {
    let manifest = tauri_build::AppManifest::new().commands(APPLICATION_COMMANDS);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("Tauri build configuration must be valid");
}
