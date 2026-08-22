#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(all(feature = "debug-local-profiles", not(debug_assertions)))]
compile_error!("debug-local-profiles is limited to debug Rust builds");

mod commands;
#[cfg(feature = "debug-local-profiles")]
mod debug_profile;
mod release_configuration;
mod startup;
mod update_service;

use tauri::Manager;

fn main() {
    // Parse before Tauri setup, identity construction, or any app-data lookup.
    // A normal binary therefore cannot reach profile storage or file custody.
    let profile_name =
        startup::profile_argument_from_environment().expect("invalid Resonance startup argument");

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(move |app| {
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
            let startup = select_workspace_startup(app, profile_name.as_ref())?;
            let workspace = commands::workspace::ManagedWorkspaceState::initialize(
                app.handle().clone(),
                startup.identity,
                &startup.application_data,
            );
            workspace.start_lifecycle();
            app.manage(workspace);
            #[cfg(feature = "debug-local-profiles")]
            if let Some(profile) = startup.profile {
                // Keep the advisory lock alive for the entire desktop lifetime.
                app.manage(profile);
            }
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
            commands::workspace::workspace_view,
            commands::workspace::create_workspace,
            commands::workspace::create_workspace_invite,
            commands::workspace::join_workspace,
            commands::workspace::retry_workspace_join
        ])
        .run(tauri::generate_context!())
        .expect("error while running Resonance");
}

struct WorkspaceStartup {
    identity: Result<
        resonance_runtime::identity::InstallationIdentity,
        resonance_runtime::identity::IdentityError,
    >,
    application_data: Box<std::path::Path>,
    #[cfg(feature = "debug-local-profiles")]
    profile: Option<debug_profile::DebugProfile>,
}

fn select_workspace_startup(
    app: &tauri::App,
    profile_name: Option<&startup::DebugProfileName>,
) -> Result<WorkspaceStartup, std::io::Error> {
    #[cfg(feature = "debug-local-profiles")]
    if let Some(profile_name) = profile_name {
        let profile =
            debug_profile::DebugProfile::open(profile_name).map_err(std::io::Error::other)?;
        let identity =
            resonance_runtime::identity::InstallationIdentity::load_or_create(profile.custody());
        return Ok(WorkspaceStartup {
            identity,
            application_data: profile.application_data().into(),
            profile: Some(profile),
        });
    }

    #[cfg(not(feature = "debug-local-profiles"))]
    let _ = profile_name;

    Ok(WorkspaceStartup {
        identity: resonance_runtime::identity::InstallationIdentity::load_or_create_native(),
        application_data: app
            .path()
            .app_data_dir()
            .map_err(std::io::Error::other)?
            .into_boxed_path(),
        #[cfg(feature = "debug-local-profiles")]
        profile: None,
    })
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
