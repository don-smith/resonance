use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use resonance_runtime::{
    workspace_catalog::WorkspaceCatalog,
    workspace_domain::{Member, WorkspaceId, WorkspaceLifecycle},
    workspace_store::DocumentMetadata,
};

fn temporary_directory(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("resonance-{name}-{nonce}"));
    fs::create_dir_all(&path).expect("temporary directory must be created");
    path
}

#[test]
fn keeps_workspace_configuration_membership_and_documents_isolated() {
    let root = temporary_directory("workspace-catalog-isolation");
    let catalog = WorkspaceCatalog::open(&root).expect("catalog opens");
    let alpha = catalog
        .create_workspace("Alpha", Some("https://relay.alpha.example".to_owned()))
        .expect("alpha creates");
    let beta = catalog
        .create_workspace("Beta", None)
        .expect("beta creates");

    let alpha_store = catalog
        .open_workspace(&alpha.id)
        .expect("alpha store opens");
    let beta_store = catalog.open_workspace(&beta.id).expect("beta store opens");
    alpha_store
        .save_document(
            &DocumentMetadata::new("plan", "Alpha plan", 1),
            "alpha",
            &[1],
        )
        .expect("alpha document saves");
    alpha_store
        .record_membership_operation("alpha-genesis", b"alpha operation")
        .expect("alpha operation saves");
    alpha_store
        .replace_members(&[Member::new(
            "alpha-public",
            "Ada",
            "developer",
            "alpha-public",
            1,
        )])
        .expect("alpha members save");

    assert_eq!(
        catalog.active_workspace().expect("active workspace"),
        Some(beta.clone())
    );
    assert_eq!(
        alpha_store.settings().expect("alpha settings").display_name,
        "Alpha"
    );
    assert_eq!(
        alpha_store
            .settings()
            .expect("alpha settings")
            .relay_override,
        Some("https://relay.alpha.example".to_owned())
    );
    assert_eq!(
        beta_store.settings().expect("beta settings").display_name,
        "Beta"
    );
    assert_eq!(
        beta_store.settings().expect("beta settings").relay_override,
        None
    );
    assert_eq!(
        beta_store
            .membership_operation_ids()
            .expect("beta operations"),
        Vec::<String>::new()
    );
    assert_eq!(
        beta_store.members().expect("beta members"),
        Vec::<Member>::new()
    );
    assert_eq!(
        beta_store.load_document("plan").expect("beta document"),
        None
    );

    catalog
        .set_active_workspace(&alpha.id)
        .expect("active workspace changes");
    assert_eq!(
        catalog.active_workspace().expect("active workspace"),
        Some(alpha.clone())
    );
    assert_eq!(alpha.lifecycle, WorkspaceLifecycle::Ready);

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn rejects_an_unknown_catalog_record_instead_of_opening_a_composed_path() {
    let root = temporary_directory("workspace-catalog-record");
    let catalog = WorkspaceCatalog::open(&root).expect("catalog opens");
    let unknown = WorkspaceId::parse(&"a".repeat(64)).expect("opaque ID parses");

    assert!(catalog.open_workspace(&unknown).is_err());
    assert!(!root
        .join(".resonance/workspaces")
        .join(unknown.as_str())
        .exists());

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}
