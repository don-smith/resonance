const RUNTIME_MANIFEST: &str = include_str!("../Cargo.toml");
const RUNTIME_MODULES: &str = include_str!("../src/lib.rs");

#[test]
fn defers_collaboration_and_other_phase_boundaries() {
    for dependency in ["iroh", "yrs", "yjs", "git2", "notify", "ollama"] {
        assert!(
            !RUNTIME_MANIFEST.contains(dependency),
            "Phase 1 must defer the {dependency} runtime dependency"
        );
    }
    for module in [
        "identity",
        "transport",
        "conversation",
        "repository",
        "agent_execution",
    ] {
        assert!(
            !RUNTIME_MODULES.contains(&format!("mod {module}")),
            "Phase 1 must defer the {module} runtime module"
        );
    }
}
