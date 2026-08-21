const RUNTIME_MANIFEST: &str = include_str!("../Cargo.toml");
const RUNTIME_MODULES: &str = include_str!("../src/lib.rs");
const IDENTITY_MODULE: &str = include_str!("../src/identity/mod.rs");
const WORKSPACE_DOMAIN: &str = include_str!("../src/workspace_domain.rs");
const DESKTOP_BOOTSTRAP: &str = include_str!("../../../apps/desktop/src-tauri/src/main.rs");
const DESKTOP_COMMANDS: &str =
    include_str!("../../../apps/desktop/src-tauri/src/commands/workspace.rs");
const PACKAGE_RUNTIME: &str = include_str!("../src/packages/mod.rs");
const PACKAGE_CONTRACT: &str = include_str!("../../../packages/contracts/src/index.ts");

#[test]
fn permits_identity_and_transport_dependencies_behind_runtime_modules() {
    for dependency in [
        "iroh = \"=1.0.3\"",
        "iroh-gossip = \"=0.101.0\"",
        "keyring = \"=4.1.6\"",
    ] {
        assert!(
            RUNTIME_MANIFEST.contains(dependency),
            "the runtime must pin {dependency}"
        );
    }
    for module in [
        "pub mod identity;",
        "pub mod workspace_catalog;",
        "pub mod workspace_domain;",
    ] {
        assert!(
            RUNTIME_MODULES.contains(module),
            "the runtime must expose its {module} boundary"
        );
    }
}

#[test]
fn keeps_private_keys_tokens_raw_transport_values_and_paths_out_of_shell_and_packages() {
    let public_boundaries = [
        DESKTOP_BOOTSTRAP,
        DESKTOP_COMMANDS,
        PACKAGE_RUNTIME,
        PACKAGE_CONTRACT,
    ]
    .join("\n");

    for forbidden in [
        "SecretKey",
        "WorkspaceToken",
        "keyring::",
        "iroh::",
        "PathBuf",
    ] {
        assert!(
            !public_boundaries.contains(forbidden),
            "desktop and package boundaries must not mention {forbidden}"
        );
    }
}

#[test]
fn keeps_custody_and_workspace_tokens_runtime_private() {
    assert!(IDENTITY_MODULE.contains("secret_key: SecretKey"));
    assert!(WORKSPACE_DOMAIN.contains("pub(crate) struct WorkspaceToken"));
    assert!(!WORKSPACE_DOMAIN.contains("pub struct WorkspaceToken"));
}
