use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    local_root_binding::{RootHealth, RootSelection},
    workspace_catalog::WorkspaceCatalog,
    workspace_file_runtime::{FileEntryKind, RootBindingStatus, WorkspaceFileRuntimeError},
    workspace_file_transport::{FileRequest, FileResponse},
    workspace_files::blobs::ContentHash,
    workspace_session::{FakeDeliveryPort, WorkspaceSession},
};

#[test]
fn owns_durable_markdown_bytes_and_reopens_the_private_root() {
    let application_data = tempfile::tempdir().expect("application data creates");
    let root = application_data.path().join("workspace-root");
    let custody = InMemoryKeyCustody::default();
    let identity =
        InstallationIdentity::load_or_create(&custody).expect("installation identity creates");
    let catalog = WorkspaceCatalog::open(application_data.path()).expect("catalog opens");
    let mut session = WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
    session
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");

    let mut files = session
        .open_file_runtime()
        .expect("file runtime opens for ready workspace");
    assert_eq!(files.root_status(), RootBindingStatus::Unbound);
    files
        .bind_root(&root, RootSelection::ConfirmedNotGitManaged)
        .expect("private root binds");
    assert_eq!(
        files.root_status(),
        RootBindingStatus::Bound(RootHealth::Healthy)
    );

    let plans = files
        .tree_entries()
        .into_iter()
        .find(|entry| entry.name == "plans" && entry.kind == FileEntryKind::Directory)
        .expect("plans directory projects");
    let created = files
        .create_markdown_file(&plans.node_id, "roadmap.md", "# Roadmap\n")
        .expect("markdown file creates");
    let opened = files
        .open_markdown_file(&created.node_id, &created.revision_id)
        .expect("markdown revision opens");
    assert_eq!(opened.markdown, "# Roadmap\n");
    assert_eq!(
        std::fs::read(root.join("plans/roadmap.md")).expect("materialized file reads"),
        b"# Roadmap\n"
    );

    drop(files);
    drop(session);

    let identity =
        InstallationIdentity::load_or_create(&custody).expect("installation identity reloads");
    let catalog = WorkspaceCatalog::open(application_data.path()).expect("catalog reopens");
    let mut restarted = WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
    restarted
        .activate_active_workspace()
        .expect("workspace activates")
        .expect("active workspace exists");
    let files = restarted
        .open_file_runtime()
        .expect("file runtime reconstructs");

    assert_eq!(
        files.root_status(),
        RootBindingStatus::Bound(RootHealth::Healthy)
    );
    assert_eq!(
        files
            .open_markdown_file(&created.node_id, &created.revision_id)
            .expect("durable revision reopens")
            .markdown,
        "# Roadmap\n"
    );
    let service = files
        .recovery_service()
        .expect("authorized recovery service composes");
    assert!(matches!(
        service.handle(
            &restarted.local_public_identity(),
            FileRequest::MissingOperations {
                workspace_id: restarted
                    .view()
                    .expect("workspace view reads")
                    .workspace
                    .id
                    .as_str()
                    .to_owned(),
                known_operation_ids: Vec::new(),
            },
        ),
        FileResponse::Operations(operations) if operations.len() == 2
    ));
    let hash = ContentHash::from_bytes(b"# Roadmap\n");
    assert!(matches!(
        service.handle(
            &restarted.local_public_identity(),
            FileRequest::BlobChunk {
                workspace_id: restarted
                    .view()
                    .expect("workspace view reads")
                    .workspace
                    .id
                    .as_str()
                    .to_owned(),
                content_hash: hash.as_str().to_owned(),
                offset: 0,
                max_bytes: 1024,
            },
        ),
        FileResponse::BlobChunk { complete: true, bytes, .. } if bytes == b"# Roadmap\n"
    ));

    drop(files);
    drop(restarted);
    std::fs::remove_dir_all(&root).expect("bound root becomes unavailable");
    let identity =
        InstallationIdentity::load_or_create(&custody).expect("installation identity reloads");
    let catalog = WorkspaceCatalog::open(application_data.path()).expect("catalog reopens");
    let mut restarted = WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
    restarted
        .activate_active_workspace()
        .expect("workspace activates");
    let mut files = restarted.open_file_runtime().expect("file runtime reopens");
    assert_eq!(
        files.root_status(),
        RootBindingStatus::Bound(RootHealth::Unavailable)
    );
    let replacement = files
        .replace_markdown_file(&created.node_id, &created.revision_id, "# Updated\n")
        .expect("unavailable root does not block authority writes");
    assert_eq!(
        files
            .open_markdown_file(&replacement.node_id, &replacement.revision_id)
            .expect("replacement remains available from private blobs")
            .markdown,
        "# Updated\n"
    );
}

#[test]
fn failed_operation_storage_does_not_change_the_live_authority() {
    let application_data = tempfile::tempdir().expect("application data creates");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("installation identity creates");
    let catalog = WorkspaceCatalog::open(application_data.path()).expect("catalog opens");
    let mut session = WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
    session
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    let workspace_id = session
        .view()
        .expect("workspace view reads")
        .workspace
        .id
        .as_str()
        .to_owned();
    let database = application_data
        .path()
        .join(".resonance/workspaces")
        .join(&workspace_id)
        .join("workspace.sqlite3");
    let failure = rusqlite::Connection::open(database).expect("failure connection opens");
    failure
        .execute_batch(
            "CREATE TRIGGER fail_file_operation_insert
             BEFORE INSERT ON workspace_file_operations
             BEGIN
               SELECT RAISE(FAIL, 'simulated file operation storage failure');
             END;",
        )
        .expect("failure trigger installs");

    let mut files = session.open_file_runtime().expect("file runtime opens");
    let plans = files
        .tree_entries()
        .into_iter()
        .find(|entry| entry.name == "plans")
        .expect("plans exists");
    let result = files.create_markdown_file(&plans.node_id, "failed.md", "not committed\n");

    assert!(matches!(result, Err(WorkspaceFileRuntimeError::Store(_))));
    assert!(!files
        .tree_entries()
        .iter()
        .any(|entry| entry.name == "failed.md"));
    assert!(files.take_pending_announcements().is_empty());

    failure
        .execute_batch("DROP TRIGGER fail_file_operation_insert;")
        .expect("failure trigger removes");
    files
        .create_markdown_file(&plans.node_id, "failed.md", "committed\n")
        .expect("same logical create retries after storage recovers");
}

#[test]
fn failed_external_change_storage_is_retried_without_losing_local_bytes() {
    let application_data = tempfile::tempdir().expect("application data creates");
    let root = application_data.path().join("workspace-root");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("installation identity creates");
    let catalog = WorkspaceCatalog::open(application_data.path()).expect("catalog opens");
    let mut session = WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
    session
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    let workspace_id = session
        .view()
        .expect("workspace view reads")
        .workspace
        .id
        .as_str()
        .to_owned();
    let mut files = session.open_file_runtime().expect("file runtime opens");
    files
        .bind_root(&root, RootSelection::ConfirmedNotGitManaged)
        .expect("root binds");
    std::fs::write(root.join("plans/external.md"), b"external bytes\n")
        .expect("external file writes");
    assert_eq!(files.poll_root_changes().expect("first observation"), 0);

    let database = application_data
        .path()
        .join(".resonance/workspaces")
        .join(&workspace_id)
        .join("workspace.sqlite3");
    let failure = rusqlite::Connection::open(database).expect("failure connection opens");
    failure
        .execute_batch(
            "CREATE TRIGGER fail_file_operation_insert
             BEFORE INSERT ON workspace_file_operations
             BEGIN
               SELECT RAISE(FAIL, 'simulated file operation storage failure');
             END;",
        )
        .expect("failure trigger installs");

    assert!(matches!(
        files.poll_root_changes(),
        Err(WorkspaceFileRuntimeError::Store(_))
    ));
    assert_eq!(
        std::fs::read(root.join("plans/external.md")).expect("external bytes remain"),
        b"external bytes\n"
    );
    assert!(!files
        .tree_entries()
        .iter()
        .any(|entry| entry.name == "external.md"));
    assert!(files.take_pending_announcements().is_empty());

    failure
        .execute_batch("DROP TRIGGER fail_file_operation_insert;")
        .expect("failure trigger removes");
    assert_eq!(files.poll_root_changes().expect("change retries"), 1);
    assert!(files
        .tree_entries()
        .iter()
        .any(|entry| entry.name == "external.md"));
}

#[test]
fn rejects_non_markdown_and_stale_revision_editor_requests() {
    let application_data = tempfile::tempdir().expect("application data creates");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("installation identity creates");
    let catalog = WorkspaceCatalog::open(application_data.path()).expect("catalog opens");
    let mut session = WorkspaceSession::new(identity, catalog, FakeDeliveryPort::default());
    session
        .create_workspace_with_creator("Team Resonance", "Ada", None)
        .expect("workspace creates");
    let mut files = session.open_file_runtime().expect("file runtime opens");
    let plans = files
        .tree_entries()
        .into_iter()
        .find(|entry| entry.name == "plans")
        .expect("plans exists");
    let created = files
        .create_markdown_file(&plans.node_id, "notes.md", "one\n")
        .expect("markdown creates");

    assert!(files
        .replace_markdown_file(&created.node_id, "not-a-revision", "two\n")
        .is_err());
    assert!(files
        .create_markdown_file(&plans.node_id, "asset.bin", "bytes")
        .is_err());
}
