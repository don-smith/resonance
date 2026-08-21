use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use resonance_runtime::workspace_store::{DocumentMetadata, WorkspaceStore};
use rusqlite::Connection;

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
fn persists_metadata_markdown_and_yjs_bytes_across_workspace_reopen() {
    let root = temporary_directory("workspace-store-roundtrip");
    let primary = WorkspaceStore::open(&root, "primary").expect("primary workspace opens");
    let secondary = WorkspaceStore::open(&root, "secondary").expect("secondary workspace opens");
    let metadata = DocumentMetadata::new("document-1", "Plan", 1_725_000_000);

    primary
        .save_document(&metadata, "# Plan\n", &[0, 255, 42])
        .expect("document saves");

    assert_eq!(
        primary.load_document("document-1").expect("document loads"),
        Some((metadata.clone(), "# Plan\n".to_owned(), vec![0, 255, 42]))
    );
    assert_eq!(
        secondary
            .load_document("document-1")
            .expect("other workspace loads"),
        None
    );

    drop(primary);
    let reopened = WorkspaceStore::open(&root, "primary").expect("workspace reopens");
    assert_eq!(
        reopened
            .load_document("document-1")
            .expect("document reloads"),
        Some((metadata, "# Plan\n".to_owned(), vec![0, 255, 42]))
    );
    assert!(workspace_database(&root, "primary").is_file());
    assert!(root
        .join(".resonance/workspaces/primary/documents/document-1.md")
        .is_file());
    assert!(root
        .join(".resonance/workspaces/primary/documents/document-1.yjs")
        .is_file());

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn migrates_a_previous_workspace_schema_fixture() {
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
        store
            .load_document("legacy-doc")
            .expect("migrated document loads"),
        Some((
            DocumentMetadata::new("legacy-doc", "Legacy document", 0),
            String::new(),
            Vec::new(),
        ))
    );

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn restores_the_last_complete_export_after_an_interrupted_replacement() {
    let root = temporary_directory("workspace-store-transaction-recovery");
    let store = WorkspaceStore::open(&root, "workspace").expect("workspace opens");
    let old_metadata = DocumentMetadata::new("document-1", "Before", 1);
    store
        .save_document(&old_metadata, "before", &[1])
        .expect("original document saves");
    drop(store);

    let documents = root.join(".resonance/workspaces/workspace/documents");
    fs::rename(
        documents.join("document-1.md"),
        documents.join("document-1.md.bak"),
    )
    .expect("original Markdown is backed up");
    fs::rename(
        documents.join("document-1.yjs"),
        documents.join("document-1.yjs.bak"),
    )
    .expect("original snapshot is backed up");
    fs::write(documents.join("document-1.md"), "partial")
        .expect("partial Markdown replacement is simulated");
    fs::write(documents.join("document-1.yjs.tmp"), [2])
        .expect("partial snapshot replacement is simulated");
    fs::write(documents.join("document-1.pending"), "pending")
        .expect("pending marker is simulated");
    Connection::open(workspace_database(&root, "workspace"))
        .expect("workspace database opens")
        .execute(
            "INSERT INTO pending_exports (document_id) VALUES (?1)",
            ["document-1"],
        )
        .expect("pending export is journaled");

    let reopened = WorkspaceStore::open(&root, "workspace").expect("workspace recovers");
    assert_eq!(
        reopened
            .load_document("document-1")
            .expect("document loads"),
        Some((old_metadata, "before".to_owned(), vec![1]))
    );

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}

#[test]
fn ignores_an_interrupted_temporary_export_when_reopened() {
    let root = temporary_directory("workspace-store-recovery");
    let store = WorkspaceStore::open(&root, "workspace").expect("workspace opens");
    let metadata = DocumentMetadata::new("document-1", "Stable", 1);
    store
        .save_document(&metadata, "complete", &[1, 2, 3])
        .expect("document saves");
    drop(store);

    let documents = root.join(".resonance/workspaces/workspace/documents");
    fs::write(documents.join("document-1.md.tmp"), "incomplete")
        .expect("interrupted temporary export is simulated");
    fs::write(documents.join("document-1.yjs.tmp"), [9, 9])
        .expect("interrupted temporary snapshot is simulated");

    let reopened = WorkspaceStore::open(&root, "workspace").expect("workspace recovers");
    assert_eq!(
        reopened
            .load_document("document-1")
            .expect("document loads"),
        Some((metadata, "complete".to_owned(), vec![1, 2, 3]))
    );
    assert!(!documents.join("document-1.md.tmp").exists());
    assert!(!documents.join("document-1.yjs.tmp").exists());

    fs::remove_dir_all(root).expect("temporary directory cleans up");
}
