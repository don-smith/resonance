use resonance_runtime::{
    identity::PublicIdentity,
    workspace_domain::{display_name, WorkspaceId},
};

#[test]
fn runtime_domain_values_validate_at_their_public_seams() {
    let identity = PublicIdentity::parse(&"ab".repeat(32)).expect("identity parses");
    assert_eq!(identity.to_string(), "ab".repeat(32));
    assert!(PublicIdentity::parse("short").is_err());

    let workspace = WorkspaceId::parse(&"cd".repeat(32)).expect("workspace ID parses");
    assert_eq!(workspace.as_str(), "cd".repeat(32));
    assert!(WorkspaceId::parse("not-a-workspace").is_err());
}

#[test]
fn workspace_display_names_are_bounded_and_normalized_once() {
    assert_eq!(
        display_name("  Team Resonance  ").expect("name validates"),
        "Team Resonance"
    );
    assert!(display_name(" ").is_err());
    assert!(display_name("x".repeat(257)).is_err());
}

#[test]
fn public_runtime_types_do_not_accept_malformed_identity_text() {
    let malformed = format!("{}z", "0".repeat(63));
    assert!(PublicIdentity::parse(&malformed).is_err());
}
