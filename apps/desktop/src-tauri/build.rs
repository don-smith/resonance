const APPLICATION_COMMANDS: &[&str] = &[
    "bundled_package_ids",
    "workspace_view",
    "workspace_files_v1",
    "choose_workspace_root",
    "replace_workspace_root",
    "repair_workspace_root",
    "unbind_workspace_root",
    "open_markdown_file",
    "open_file_preview",
    "create_markdown_file",
    "replace_markdown_file",
    "resolve_workspace_conflict",
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
