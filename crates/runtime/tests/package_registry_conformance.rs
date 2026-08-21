use resonance_runtime::packages::{PackageRegistry, PackageSource};

const VALID: &str =
    include_str!("../../../packages/contracts/fixtures/valid/reference-manifest.json");
const INVALID_SOURCE: &str =
    include_str!("../../../packages/contracts/fixtures/invalid/placeholder-source.json");
const INVALID_PERMISSION: &str =
    include_str!("../../../packages/contracts/fixtures/invalid/unknown-permission.json");

#[test]
fn accepts_the_shared_valid_conformance_fixture() {
    let registry = PackageRegistry::load(PackageSource::BundledTeam, &[VALID])
        .expect("shared valid fixture must load");

    assert!(registry.get("resonance.reference").is_some());
}

#[test]
fn rejects_shared_invalid_fixtures_with_actionable_diagnostics() {
    for (fixture, expected_message) in [
        (INVALID_SOURCE, "source must be bundled-team"),
        (
            INVALID_PERMISSION,
            "unsupported agent permission: filesystem.read",
        ),
    ] {
        let diagnostics = PackageRegistry::load(PackageSource::BundledTeam, &[fixture])
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
        "only bundled-team packages may load in Phase 1"
    );

    let collision_diagnostics = PackageRegistry::load(PackageSource::BundledTeam, &[VALID, VALID])
        .expect_err("duplicate id must not load");
    assert!(collision_diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message
            == "namespace collision: package id is already registered"));
}
