#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
    tauri::Builder::default()
        .setup(|_app| {
            let _runtime_name = resonance_runtime::runtime_name();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::packages::bundled_package_ids
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
