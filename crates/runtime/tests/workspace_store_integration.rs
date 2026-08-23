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
fn migrates_a_previous_workspace_schema_to_the_filesystem_authority_marker() {
    let root = temporary_directory("workspace-store-migration");
    let database = workspace_database(&root, "legacy");
    fs::create_dir_all(database.parent().expect("database has parent"))
        .expect("workspace directory creates");
    let connection = Connection::open(&database).expect("legacy database opens");
    connection
        .execute_batch(include_str!("fixtures/legacy_workspace_v0.sql"))
        .expect("legacy fixture applies");
    drop(connection);

    let store = WorkspaceStore::open(&root, "legacy").expect("legacy workspace migrates");
    assert_eq!(
        store.initial_root_name().expect("filesystem root reads"),
        "plans"
    );

    let connection = Connection::open(&database).expect("migrated database opens");
    let version: i32 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .expect("schema version reads");
    assert_eq!(version, 8);
    let file_history_table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'workspace_file_operations')",
            [],
            |row| row.get(0),
        )
        .expect("file history table check succeeds");
    assert!(file_history_table_exists);
    let root_binding_table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'local_root_binding')",
            [],
            |row| row.get(0),
        )
        .expect("local root table check succeeds");
    assert!(root_binding_table_exists);
    let documents_table_exists: bool = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'documents')",
            [],
            |row| row.get(0),
        )
        .expect("documents table check succeeds");
    assert!(!documents_table_exists);

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}
