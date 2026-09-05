use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use resonance_runtime::{
    identity::{InMemoryKeyCustody, InstallationIdentity},
    workspace_files::SignedFileOperation,
    workspace_store::WorkspaceStore,
};
use rusqlite::{params, Connection};

fn temporary_directory(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must be after Unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("resonance-{name}-{nonce}"));
    fs::create_dir_all(&path).expect("temporary directory must be created");
    path
}

fn workspace_database(root: &Path, workspace_id: &str) -> PathBuf {
    root.join(".resonance")
        .join("workspaces")
        .join(workspace_id)
        .join("workspace.sqlite3")
}

#[test]
fn creates_a_clean_filesystem_workspace_marker() {
    let root = temporary_directory("workspace-store-file-authority");
    let store = WorkspaceStore::open(&root, "primary").expect("workspace opens");

    assert_eq!(
        store.initial_root_name().expect("initial root reads"),
        "plans"
    );
    assert!(workspace_database(&root, "primary").is_file());
    assert!(!root
        .join(".resonance/workspaces/primary/documents")
        .exists());

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn rolls_back_a_file_operation_batch_when_any_insert_fails() {
    let root = temporary_directory("workspace-store-operation-batch");
    let store = WorkspaceStore::open(&root, "primary").expect("workspace opens");
    let identity = InstallationIdentity::load_or_create(&InMemoryKeyCustody::default())
        .expect("identity creates");
    let first =
        SignedFileOperation::create_directory(&identity, "primary", None, "plans", Vec::new())
            .expect("first operation signs");
    let second = SignedFileOperation::create_directory(
        &identity,
        "primary",
        None,
        "archive",
        vec![first.operation.operation_id.clone()],
    )
    .expect("second operation signs");
    let database = workspace_database(&root, "primary");
    let failure = Connection::open(&database).expect("failure connection opens");
    failure
        .execute_batch(&format!(
            "CREATE TRIGGER fail_second_file_operation
             BEFORE INSERT ON workspace_file_operations
             WHEN NEW.operation_id = '{}'
             BEGIN
               SELECT RAISE(FAIL, 'simulated second insert failure');
             END;",
            second.operation.operation_id
        ))
        .expect("failure trigger installs");

    assert!(store
        .record_file_operations(&[first.clone(), second])
        .is_err());
    let persisted: i64 = failure
        .query_row(
            "SELECT COUNT(*) FROM workspace_file_operations WHERE operation_id = ?1",
            params![first.operation.operation_id],
            |row| row.get(0),
        )
        .expect("persisted count reads");
    assert_eq!(persisted, 0);

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn rejects_an_unsupported_legacy_workspace_without_deleting_user_data() {
    let root = temporary_directory("workspace-store-migration");
    let database = workspace_database(&root, "legacy");
    fs::create_dir_all(database.parent().expect("database has parent"))
        .expect("workspace directory creates");
    let connection = Connection::open(&database).expect("legacy database opens");
    connection
        .execute_batch(include_str!("fixtures/legacy_workspace_v0.sql"))
        .expect("legacy fixture applies");
    drop(connection);

    let error = WorkspaceStore::open(&root, "legacy")
        .expect_err("unsupported legacy workspace must be rejected");
    assert!(matches!(
        error,
        resonance_runtime::workspace_store::WorkspaceStoreError::UnsupportedLegacySchema
    ));

    let connection = Connection::open(&database).expect("legacy database remains open");
    let title: String = connection
        .query_row(
            "SELECT title FROM documents WHERE id = 'legacy-doc'",
            [],
            |row| row.get(0),
        )
        .expect("legacy document remains");
    assert_eq!(title, "Legacy document");

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn upgrades_the_supported_workspace_v8_fixture_transactionally() {
    let root = temporary_directory("workspace-store-v8-fixture");
    let database = workspace_database(&root, "legacy-v8");
    fs::create_dir_all(database.parent().expect("database has parent"))
        .expect("workspace directory creates");
    let connection = Connection::open(&database).expect("legacy database opens");
    connection
        .execute_batch(include_str!("fixtures/legacy_workspace_v8.sql"))
        .expect("v8 fixture applies");
    drop(connection);

    let store = WorkspaceStore::open(&root, "legacy-v8").expect("supported workspace upgrades");
    assert_eq!(
        store.settings().expect("settings read").display_name,
        "Legacy workspace"
    );
    let connection = Connection::open(database).expect("upgraded database opens");
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("workspace version reads");
    assert_eq!(version, 11);
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn upgrades_schema_v10_without_losing_workspace_or_file_data() {
    let root = temporary_directory("workspace-store-v10-fixture");
    let database = workspace_database(&root, "legacy-v10");
    fs::create_dir_all(database.parent().expect("database has parent"))
        .expect("workspace directory creates");
    Connection::open(&database)
        .expect("legacy database opens")
        .execute_batch(include_str!("fixtures/legacy_workspace_v10.sql"))
        .expect("v10 fixture applies");

    let store = WorkspaceStore::open(&root, "legacy-v10").expect("v10 workspace upgrades");
    assert_eq!(
        store.settings().expect("settings").display_name,
        "Legacy workspace"
    );
    let connection = Connection::open(database).expect("upgraded database opens");
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i32>(0))
            .expect("schema version"),
        11
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT signed_operation FROM workspace_file_operations WHERE operation_id = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'",
                [],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .expect("file row remains"),
        vec![1, 2, 3]
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM membership_operations", [], |row| {
                row.get::<_, i64>(0)
            })
            .expect("membership row remains"),
        1
    );
    assert_eq!(
        store.lookup_peer_set_version().expect("conversation state"),
        0
    );
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn completes_conversation_state_for_an_existing_phase3_schema_v11_workspace() {
    let root = temporary_directory("workspace-store-phase3-v11");
    let database = workspace_database(&root, "phase3-v11");
    fs::create_dir_all(database.parent().expect("database has parent"))
        .expect("workspace directory creates");
    let connection = Connection::open(&database).expect("database opens");
    connection
        .execute_batch(include_str!("fixtures/legacy_workspace_v10.sql"))
        .expect("v10 fixture applies");
    connection
        .execute_batch(include_str!("../migrations/0011_conversations.sql"))
        .expect("phase3 schema applies");
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("version");
    assert_eq!(version, 11);
    assert!(connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'conversation_message_archive'",
            [],
            |_| Ok(()),
        )
        .is_err());
    drop(connection);

    let store = WorkspaceStore::open(&root, "phase3-v11").expect("phase3 workspace completes");
    assert_eq!(
        store.settings().expect("settings").display_name,
        "Legacy workspace"
    );
    let connection = Connection::open(database).expect("database reopens");
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM conversation_message_archive",
                [],
                |row| row.get::<_, i64>(0),
            )
            .expect("conversation archive exists"),
        0
    );
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn rolls_back_workspace_migration_when_legacy_rows_violate_new_invariants() {
    let root = temporary_directory("workspace-store-rollback");
    let database = workspace_database(&root, "legacy-invalid");
    fs::create_dir_all(database.parent().expect("database has parent"))
        .expect("workspace directory creates");
    let connection = Connection::open(&database).expect("legacy database opens");
    connection
        .execute_batch(include_str!("fixtures/legacy_workspace_v8.sql"))
        .expect("v8 fixture applies");
    connection
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO local_root_materialization
               (node_id, relative_path, revision_id, content_hash)
               VALUES ('node', 'plans/file.md', 'revision', NULL);",
        )
        .expect("invalid legacy row writes");
    drop(connection);

    assert!(WorkspaceStore::open(&root, "legacy-invalid").is_err());
    let connection = Connection::open(database).expect("legacy database remains open");
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("workspace version reads");
    let rows: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM local_root_materialization",
            [],
            |row| row.get(0),
        )
        .expect("legacy rows remain");
    assert_eq!(version, 8);
    assert_eq!(rows, 1);
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn sqlite_rejects_impossible_operation_join_and_materialization_states() {
    let root = temporary_directory("workspace-store-all-invariants");
    let store = WorkspaceStore::open(&root, "invariants").expect("store opens");
    let database = workspace_database(&root, "invariants");
    let connection = Connection::open(database).expect("database opens");
    assert!(connection
        .execute(
            "INSERT INTO membership_operations (operation_id, signed_operation)
             VALUES ('short', X'01')",
            [],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO workspace_file_operations (operation_id, signed_operation)
             VALUES ('short', X'01')",
            [],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO local_root_materialization
             (node_id, relative_path, revision_id, content_hash)
             VALUES ('node', 'plans/file.md', 'revision', NULL)",
            [],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO local_root_materialization
             (node_id, relative_path, revision_id, content_hash)
             VALUES ('node', 'plans/file.md', NULL, 'not-a-hash')",
            [],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO local_root_binding (singleton, root_path, health)
             VALUES (1, '/tmp/plans', 'unknown')",
            [],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO workspace_configuration
             (singleton, token, display_name, lifecycle, joining_inviter)
             VALUES (1, zeroblob(32), 'Team', 'ready', zeroblob(32))",
            [],
        )
        .is_err());
    assert!(connection
        .execute(
            "INSERT INTO workspace_configuration
             (singleton, token, display_name, lifecycle)
             VALUES (1, zeroblob(32), 'Team', 'unknown')",
            [],
        )
        .is_err());
    drop(store);
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn typed_store_reads_report_corrupt_persisted_values() {
    let root = temporary_directory("workspace-store-corruption");
    let store = WorkspaceStore::open(&root, "corrupt").expect("store opens");
    let database = workspace_database(&root, "corrupt");
    let connection = Connection::open(database).expect("database opens");
    connection
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             INSERT INTO workspace_configuration
               (singleton, token, display_name, lifecycle)
               VALUES (1, zeroblob(32), 'Corrupt', 'impossible');",
        )
        .expect("corrupt row writes");
    assert!(matches!(
        store.settings(),
        Err(
            resonance_runtime::workspace_store::WorkspaceStoreError::CorruptPersistedValue(
                "lifecycle"
            )
        )
    ));
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn departure_request_and_retry_duty_commit_before_first_transmission() {
    let root = temporary_directory("workspace-store-departure-duty");
    let workspace = "b1".repeat(32);
    let store = WorkspaceStore::open(&root, &workspace).expect("store opens");
    let requester =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![71; 32]))
            .expect("requester loads");
    let creator =
        InstallationIdentity::load_or_create(&InMemoryKeyCustody::with_secret(vec![72; 32]))
            .expect("creator loads");
    let request = resonance_runtime::membership_log::SignedSelfRemovalRequestV1::create_with_nonce(
        &requester,
        &workspace,
        "c1".repeat(32),
        *creator.public_identity().as_bytes(),
        [73; 32],
        1,
    )
    .expect("request signs");

    let first = store
        .record_departure_request(&request)
        .expect("request and duty persist atomically");
    let second = store
        .record_departure_request(&request)
        .expect("duplicate request is idempotent");

    assert_eq!(first, second);
    assert_eq!(
        store.pending_departure_requests().expect("requests").len(),
        1
    );
    let duties = store.durable_publication_duties().expect("duties");
    assert_eq!(duties.len(), 1);
    assert_eq!(duties[0].transport, "iroh-departure");
    assert_eq!(
        duties[0].exact_bytes,
        request.encode().expect("request encodes")
    );
    drop(store);
    let reopened = WorkspaceStore::open(&root, &workspace).expect("store reopens");
    assert_eq!(
        reopened
            .pending_departure_requests()
            .expect("requests")
            .len(),
        1
    );
    assert_eq!(
        reopened.durable_publication_duties().expect("duties").len(),
        1
    );
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn sqlite_rejects_impossible_workspace_member_values_without_interpreting_roles() {
    let root = temporary_directory("workspace-store-invariants");
    let store = WorkspaceStore::open(&root, "invariants").expect("store opens");
    let database = workspace_database(&root, "invariants");
    let connection = Connection::open(database).expect("database opens");
    let error = connection.execute(
        "INSERT INTO workspace_members
         (public_identity, display_name, role, added_by, added_at)
         VALUES (?1, 'Ada', '', ?1, 1)",
        [&"a".repeat(64)],
    );
    assert!(error.is_err());
    assert!(connection
        .execute(
            "INSERT INTO workspace_members
             (public_identity, display_name, role, added_by, added_at)
             VALUES (?1, 'Ada', 'workspace-admin', ?1, 1)",
            [&"b".repeat(64)],
        )
        .is_ok());
    drop(store);
    fs::remove_dir_all(root).expect("temporary directory cleans up");
}
