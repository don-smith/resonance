#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod release_configuration;
mod update_service;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let _runtime_name = resonance_runtime::runtime_name();
            if let Some(configuration) =
                release_configuration::load_for_current_run(app).map_err(std::io::Error::other)?
            {
                // Constructing the updater validates the Tauri handoff without
                // performing an update check during application startup.
                let _updater = update_service::UpdateService::new(configuration)
                    .updater(app.handle())
                    .map_err(std::io::Error::other)?;
            }
            let application_data = app.path().app_data_dir()?;
            commands::workspace::bootstrap_default_workspace(&application_data)
                .map_err(std::io::Error::other)?;
            if let Some(window) = app.get_webview_window("main") {
                window.center()?;
                window.unminimize()?;
                window.show()?;
                window.set_focus()?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::packages::bundled_package_ids,
            commands::workspace::default_workspace_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}

#[cfg(test)]
mod tests {
    const TAURI_CONFIGURATION: &str = include_str!("../tauri.conf.json");

    #[test]
    fn keeps_the_unprovisioned_updater_plugin_deserializable() {
        let configuration: serde_json::Value =
            serde_json::from_str(TAURI_CONFIGURATION).expect("Tauri configuration is valid JSON");
        let updater = &configuration["plugins"]["updater"];

        assert_eq!(updater["pubkey"], "");
        assert_eq!(updater["endpoints"], serde_json::json!([]));
    }
}
