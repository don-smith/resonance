#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let _runtime_name = resonance_runtime::runtime_name();
            let application_data = app.path().app_data_dir()?;
            commands::workspace::bootstrap_default_workspace(&application_data)
                .map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::packages::bundled_package_ids,
            commands::workspace::default_workspace_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
