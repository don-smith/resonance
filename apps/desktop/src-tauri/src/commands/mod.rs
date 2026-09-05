pub mod conversations;
pub mod packages;
pub mod workspace;
pub mod workspace_files;

/// The native application-command registry. Build-time ACL validation and
/// handler registration both consume this declaration.
pub const APPLICATION_COMMANDS: &[&str] = &[
    "workspace_view",
    "workspace_files_v1",
    "conversations_v1",
    "create_workspace",
    "create_workspace_invite",
    "join_workspace",
    "retry_workspace_join",
];

macro_rules! application_handler {
    () => {
        tauri::generate_handler![
            commands::workspace::workspace_view,
            commands::workspace_files::workspace_files_v1,
            commands::conversations::conversations_v1,
            commands::workspace::create_workspace,
            commands::workspace::create_workspace_invite,
            commands::workspace::join_workspace,
            commands::workspace::retry_workspace_join
        ]
    };
}

pub(crate) use application_handler;
