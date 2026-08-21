//! Workspace-scoped durable document storage.
//!
//! Callers provide only an application-data directory and opaque domain values.
//! SQLite, filesystem layout, migrations, and interrupted-write recovery remain
//! internal to this module.

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, Connection};

const CURRENT_SCHEMA_VERSION: i32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DocumentMetadata {
    pub id: String,
    pub title: String,
    pub updated_at: i64,
}

impl DocumentMetadata {
    #[must_use]
    pub fn new(id: impl Into<String>, title: impl Into<String>, updated_at: i64) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            updated_at,
        }
    }
}

pub type StoredDocument = (DocumentMetadata, String, Vec<u8>);

#[derive(Debug)]
pub struct WorkspaceStore {
    directory: PathBuf,
    connection: Mutex<Connection>,
}

#[derive(Debug)]
pub enum WorkspaceStoreError {
    InvalidIdentifier(&'static str),
    Io(std::io::Error),
    Database(rusqlite::Error),
    LockPoisoned,
}

impl std::fmt::Display for WorkspaceStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentifier(kind) => write!(formatter, "invalid {kind} identifier"),
            Self::Io(error) => write!(formatter, "workspace storage I/O failed: {error}"),
            Self::Database(error) => write!(formatter, "workspace database failed: {error}"),
            Self::LockPoisoned => formatter.write_str("workspace store lock was poisoned"),
        }
    }
}

impl std::error::Error for WorkspaceStoreError {}

impl From<std::io::Error> for WorkspaceStoreError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rusqlite::Error> for WorkspaceStoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl WorkspaceStore {
    /// Opens one opaque workspace below the supplied application-data directory.
    pub fn open(
        application_data_directory: impl AsRef<Path>,
        workspace_id: &str,
    ) -> Result<Self, WorkspaceStoreError> {
        validate_identifier(workspace_id, "workspace")?;
        let directory = application_data_directory
            .as_ref()
            .join(".resonance")
            .join("workspaces")
            .join(workspace_id);
        let documents = directory.join("documents");
        fs::create_dir_all(&documents)?;

        let connection = Connection::open(directory.join("workspace.sqlite3"))?;
        migrate(&connection)?;
        recover_interrupted_exports(&documents, &connection)?;
        Ok(Self {
            directory,
            connection: Mutex::new(connection),
        })
    }

    /// Saves all three document domains. A pending marker makes an interrupted
    /// filesystem replacement recover to the previous complete export.
    pub fn save_document(
        &self,
        metadata: &DocumentMetadata,
        markdown: &str,
        yjs_snapshot: &[u8],
    ) -> Result<(), WorkspaceStoreError> {
        validate_identifier(&metadata.id, "document")?;
        let documents = self.directory.join("documents");
        let marker = documents.join(format!("{}.pending", metadata.id));
        let markdown_path = documents.join(format!("{}.md", metadata.id));
        let snapshot_path = documents.join(format!("{}.yjs", metadata.id));
        let markdown_backup = documents.join(format!("{}.md.bak", metadata.id));
        let snapshot_backup = documents.join(format!("{}.yjs.bak", metadata.id));

        {
            let connection = self
                .connection
                .lock()
                .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
            if marker.exists() {
                recover_document(&documents, &metadata.id, &connection)?;
            } else {
                clear_pending_export(&connection, &metadata.id)?;
            }
            record_pending_export(&connection, &metadata.id)?;
        }
        fs::write(&marker, b"pending")?;
        let result = (|| -> Result<(), WorkspaceStoreError> {
            backup_if_present(&markdown_path, &markdown_backup)?;
            backup_if_present(&snapshot_path, &snapshot_backup)?;
            replace_atomically(&markdown_path, markdown.as_bytes())?;
            replace_atomically(&snapshot_path, yjs_snapshot)?;

            let mut connection = self
                .connection
                .lock()
                .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
            let transaction = connection.transaction()?;
            transaction.execute(
                "INSERT INTO documents (id, title, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(id) DO UPDATE SET title = excluded.title, updated_at = excluded.updated_at",
                params![metadata.id, metadata.title, metadata.updated_at],
            )?;
            transaction.execute(
                "DELETE FROM pending_exports WHERE document_id = ?1",
                [&metadata.id],
            )?;
            transaction.commit()?;
            Ok(())
        })();

        if result.is_ok() {
            remove_if_present(&markdown_backup)?;
            remove_if_present(&snapshot_backup)?;
            remove_if_present(&marker)?;
        }
        result
    }

    /// Loads a document only when its metadata has been committed to SQLite.
    pub fn load_document(
        &self,
        document_id: &str,
    ) -> Result<Option<StoredDocument>, WorkspaceStoreError> {
        validate_identifier(document_id, "document")?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let metadata = connection
            .query_row(
                "SELECT id, title, updated_at FROM documents WHERE id = ?1",
                [document_id],
                |row| {
                    Ok(DocumentMetadata::new(
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        drop(connection);

        let Some(metadata) = metadata else {
            return Ok(None);
        };
        let documents = self.directory.join("documents");
        let markdown = read_or_default(documents.join(format!("{document_id}.md")))?;
        let yjs_snapshot = read_or_default(documents.join(format!("{document_id}.yjs")))?;
        Ok(Some((
            metadata,
            String::from_utf8_lossy(&markdown).into_owned(),
            yjs_snapshot,
        )))
    }
}

fn migrate(connection: &Connection) -> Result<(), WorkspaceStoreError> {
    let version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 && table_exists(connection, "documents")? {
        connection.execute_batch(
            "ALTER TABLE documents ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
             PRAGMA user_version = 1;",
        )?;
    } else if version == 0 {
        connection.execute_batch(include_str!("../../migrations/0001_document_metadata.sql"))?;
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS pending_exports (
           document_id TEXT PRIMARY KEY NOT NULL
         );",
    )?;
    if CURRENT_SCHEMA_VERSION != 1 {
        return Err(WorkspaceStoreError::InvalidIdentifier("schema"));
    }
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, WorkspaceStoreError> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn recover_interrupted_exports(
    documents: &Path,
    connection: &Connection,
) -> Result<(), WorkspaceStoreError> {
    let mut markers = Vec::new();
    for entry in fs::read_dir(documents)? {
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some(id) = name.strip_suffix(".pending") {
            markers.push(id.to_owned());
        } else if name.ends_with(".tmp") {
            remove_if_present(&path)?;
        }
    }

    for id in &markers {
        if pending_export_exists(connection, id)? {
            recover_document(documents, id, connection)?;
        } else {
            finish_committed_export(documents, id)?;
        }
    }
    for id in pending_export_ids(connection)? {
        if !markers.contains(&id) {
            clear_pending_export(connection, &id)?;
        }
    }
    Ok(())
}

fn recover_document(
    documents: &Path,
    id: &str,
    connection: &Connection,
) -> Result<(), WorkspaceStoreError> {
    for extension in ["md", "yjs"] {
        let destination = documents.join(format!("{id}.{extension}"));
        let backup = documents.join(format!("{id}.{extension}.bak"));
        let temporary = documents.join(format!("{id}.{extension}.tmp"));
        if backup.exists() {
            remove_if_present(&destination)?;
            fs::rename(&backup, &destination)?;
        } else {
            remove_if_present(&destination)?;
        }
        remove_if_present(&temporary)?;
    }
    remove_if_present(&documents.join(format!("{id}.pending")))?;
    clear_pending_export(connection, id)
}

fn finish_committed_export(documents: &Path, id: &str) -> Result<(), WorkspaceStoreError> {
    for extension in ["md", "yjs"] {
        remove_if_present(&documents.join(format!("{id}.{extension}.tmp")))?;
        remove_if_present(&documents.join(format!("{id}.{extension}.bak")))?;
    }
    remove_if_present(&documents.join(format!("{id}.pending")))
}

fn record_pending_export(connection: &Connection, id: &str) -> Result<(), WorkspaceStoreError> {
    connection.execute(
        "INSERT OR REPLACE INTO pending_exports (document_id) VALUES (?1)",
        [id],
    )?;
    Ok(())
}

fn clear_pending_export(connection: &Connection, id: &str) -> Result<(), WorkspaceStoreError> {
    connection.execute("DELETE FROM pending_exports WHERE document_id = ?1", [id])?;
    Ok(())
}

fn pending_export_exists(connection: &Connection, id: &str) -> Result<bool, WorkspaceStoreError> {
    Ok(connection
        .query_row(
            "SELECT 1 FROM pending_exports WHERE document_id = ?1",
            [id],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn pending_export_ids(connection: &Connection) -> Result<Vec<String>, WorkspaceStoreError> {
    let mut statement = connection.prepare("SELECT document_id FROM pending_exports")?;
    let ids = statement
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(ids)
}

fn backup_if_present(source: &Path, backup: &Path) -> Result<(), WorkspaceStoreError> {
    if source.exists() {
        fs::rename(source, backup)?;
    }
    Ok(())
}

fn replace_atomically(destination: &Path, contents: &[u8]) -> Result<(), WorkspaceStoreError> {
    let temporary = destination.with_extension(format!(
        "{}.tmp",
        destination
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default()
    ));
    fs::write(&temporary, contents)?;
    fs::rename(temporary, destination)?;
    Ok(())
}

fn read_or_default(path: PathBuf) -> Result<Vec<u8>, WorkspaceStoreError> {
    match fs::read(path) {
        Ok(contents) => Ok(contents),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(error.into()),
    }
}

fn remove_if_present(path: &Path) -> Result<(), WorkspaceStoreError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_identifier(value: &str, kind: &'static str) -> Result<(), WorkspaceStoreError> {
    if value.is_empty()
        || !value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
    {
        return Err(WorkspaceStoreError::InvalidIdentifier(kind));
    }
    Ok(())
}

use rusqlite::OptionalExtension;
