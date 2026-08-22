use resonance_runtime::packages::{BusOutcome, PackageBus, PackageRegistry, PackageSource};

const VALID: &str =
    include_str!("../../../packages/contracts/fixtures/valid/reference-manifest.json");

#[test]
fn routes_declared_events_without_interpreting_payloads() {
    let registry =
        PackageRegistry::load(PackageSource::BundledTeam, &[VALID]).expect("valid manifest");
    let manifest = registry
        .get("resonance.reference")
        .expect("reference manifest");
    let bus = PackageBus::new(true, &[manifest]);

    assert_eq!(
        bus.emit(
            "resonance.reference",
            "doc:opened",
            br#"{"unexpected":"shape"}"#
        ),
        BusOutcome::Routed
    );
    assert_eq!(
        bus.emit(
            "resonance.reference",
            "peer:connection",
            br#"{"privateKey":"not interpreted"}"#
        ),
        BusOutcome::Routed
    );
}

#[test]
fn undeclared_events_are_rejected_in_development_and_dropped_in_production() {
    let registry =
        PackageRegistry::load(PackageSource::BundledTeam, &[VALID]).expect("valid manifest");
    let manifest = registry
        .get("resonance.reference")
        .expect("reference manifest");

    assert_eq!(
        PackageBus::new(true, &[manifest]).emit("resonance.reference", "repo:changed", b""),
        BusOutcome::Rejected {
            warning: "resonance.reference attempted undeclared emit: repo:changed".to_owned(),
        }
    );
    assert_eq!(
        PackageBus::new(false, &[manifest]).emit("resonance.reference", "repo:changed", b""),
        BusOutcome::Dropped
    );
}
