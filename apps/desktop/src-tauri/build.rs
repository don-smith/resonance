use std::{fs, path::Path};

const COMMAND_REGISTRY: &str = include_str!("src/commands/mod.rs");
const CAPABILITY: &str = include_str!("capabilities/main-shell.json");

fn registered_commands() -> Vec<&'static str> {
    let body = COMMAND_REGISTRY
        .split("pub const APPLICATION_COMMANDS: &[&str] = &[")
        .nth(1)
        .and_then(|value| value.split("];\n\nmacro_rules!").next())
        .expect("application command registry has the expected shape");
    body.lines()
        .filter_map(|line| line.trim().strip_prefix('"'))
        .filter_map(|line| line.strip_suffix("\","))
        .collect()
}

fn validate_capability(commands: &[&str]) {
    let capability: serde_json::Value =
        serde_json::from_str(CAPABILITY).expect("main shell capability must be valid JSON");
    let permissions = capability["permissions"]
        .as_array()
        .expect("main shell capability permissions must be an array");
    for command in commands {
        let permission = format!("allow-{}", command.replace('_', "-"));
        assert!(
            permissions.iter().any(|value| value == &permission),
            "main shell capability is missing {permission}"
        );
    }
    assert_eq!(
        permissions.len(),
        commands.len() + 3,
        "main shell capability has an undeclared application permission"
    );
}

fn main() {
    let commands: &'static [&'static str] = Box::leak(registered_commands().into_boxed_slice());
    validate_capability(commands);
    let manifest = tauri_build::AppManifest::new().commands(commands);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("Tauri build configuration must be valid");

    println!("cargo:rerun-if-changed=src/commands/mod.rs");
    println!("cargo:rerun-if-changed=capabilities/main-shell.json");
    let _ = fs::metadata(Path::new("src/commands/mod.rs"));
}
