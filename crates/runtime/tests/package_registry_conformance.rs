use resonance_runtime::packages::{PackageRegistry, PackageSource};

const VALID: &str =
    include_str!("../../../packages/contracts/fixtures/manifest-v2/valid/reference-manifest.json");
const INVALID_SOURCE: &str = include_str!(
    "../../../packages/contracts/fixtures/manifest-v2/invalid/placeholder-source.json"
);
const INVALID_PERMISSION: &str = include_str!(
    "../../../packages/contracts/fixtures/manifest-v2/invalid/unknown-permission.json"
);
const INVALID_ENTRY: &str =
    include_str!("../../../packages/contracts/fixtures/manifest-v2/invalid/traversing-entry.json");

#[test]
fn accepts_the_shared_valid_conformance_fixture() {
    let registry =
        PackageRegistry::load(PackageSource::Bundled, &[VALID]).expect("valid fixture must load");

    let manifest = registry
        .get("resonance.reference")
        .expect("reference manifest");
    assert_eq!(manifest.content.entry, "src/index.ts");
    assert!(manifest
        .capabilities
        .contains(&"workspace-files:v1".to_owned()));
}

#[test]
fn accepts_a_catalog_of_shared_manifests() {
    let catalog = format!("[{VALID}]");
    let registry = PackageRegistry::load_catalog(PackageSource::Bundled, &catalog)
        .expect("valid catalog must load");

    assert_eq!(registry.ids().collect::<Vec<_>>(), ["resonance.reference"]);
}

#[test]
fn rejects_shared_invalid_fixtures_with_actionable_diagnostics() {
    for (fixture, expected_message) in [
        (INVALID_SOURCE, "source must be bundled"),
        (
            INVALID_PERMISSION,
            "unsupported agent permission: filesystem.read",
        ),
        (
            INVALID_ENTRY,
            "content entry must be a package-relative TypeScript path without traversal",
        ),
    ] {
        let diagnostics = PackageRegistry::load(PackageSource::Bundled, &[fixture])
            .expect_err("invalid fixture must not load");
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message == expected_message));
    }
}

#[test]
fn rejects_non_bundled_sources_and_namespace_collisions() {
    let source_diagnostics = PackageRegistry::load(PackageSource::MemberLocal, &[VALID])
        .expect_err("member loader is deferred");
    assert_eq!(
        source_diagnostics[0].message,
        "only bundled packages may load from the bundled catalog"
    );

    let collision_diagnostics = PackageRegistry::load(PackageSource::Bundled, &[VALID, VALID])
        .expect_err("duplicate id must not load");
    assert!(collision_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message
            == "namespace collision: package id is already registered"));
}
