#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .setup(|_app| {
            let _runtime_name = resonance_runtime::runtime_name();
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}
