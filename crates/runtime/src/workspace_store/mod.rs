//! Workspace-scoped durable file-authority storage.
//!
//! Callers provide only an application-data directory and opaque domain values.
//! SQLite, filesystem layout, migrations, and interrupted-write recovery remain
//! internal to this module.

use std::{path::Path, sync::Mutex};

use rusqlite::{params, Connection};

use crate::{
    local_root_binding::{MaterializedRecord, RootHealth},
    workspace_domain::{Member, WorkspaceLifecycle, WorkspaceSettings, WorkspaceToken},
    workspace_files::{FileOperationError, SignedFileOperation},
};

const CURRENT_SCHEMA_VERSION: i32 = 8;

#[derive(Clone)]
pub(crate) struct PrivateWorkspaceSettings {
    pub token: WorkspaceToken,
    pub display_name: String,
    pub relay_override: Option<String>,
    pub joining_inviter: Option<[u8; 32]>,
    pub bootstrap: Option<String>,
    pub joining_display_name: Option<String>,
    pub creation_creator_display_name: Option<String>,
}

#[derive(Debug)]
pub struct WorkspaceStore {
    connection: Mutex<Connection>,
}

#[derive(Debug)]
pub enum WorkspaceStoreError {
    InvalidIdentifier(&'static str),
    WorkspaceConfigurationMissing,
    Io(std::io::Error),
    Database(rusqlite::Error),
    FileOperation(FileOperationError),
    LockPoisoned,
}

impl std::fmt::Display for WorkspaceStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentifier(kind) => write!(formatter, "invalid {kind} identifier"),
            Self::WorkspaceConfigurationMissing => {
                formatter.write_str("workspace configuration has not been initialized")
            }
            Self::Io(error) => write!(formatter, "workspace storage I/O failed: {error}"),
            Self::Database(error) => write!(formatter, "workspace database failed: {error}"),
            Self::FileOperation(error) => {
                write!(formatter, "workspace file operation failed: {error}")
            }
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

impl From<FileOperationError> for WorkspaceStoreError {
    fn from(error: FileOperationError) -> Self {
        Self::FileOperation(error)
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
        std::fs::create_dir_all(&directory)?;

        let connection = Connection::open(directory.join("workspace.sqlite3"))?;
        migrate(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub(crate) fn initialize_workspace(
        &self,
        token: &WorkspaceToken,
        display_name: &str,
        relay_override: Option<&str>,
        lifecycle: &WorkspaceLifecycle,
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO workspace_configuration (singleton, token, display_name, relay_override, lifecycle)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(singleton) DO NOTHING",
            params![
                token.as_bytes().as_slice(),
                display_name,
                relay_override,
                lifecycle.as_str()
            ],
        )?;
        Ok(())
    }

    /// Returns the configuration that is safe to show outside the runtime.
    pub(crate) fn private_settings(&self) -> Result<PrivateWorkspaceSettings, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection
            .query_row(
                "SELECT token, display_name, relay_override, joining_inviter, bootstrap, joining_display_name, creation_creator_display_name FROM workspace_configuration WHERE singleton = 1",
                [],
                |row| {
                    let token: Vec<u8> = row.get(0)?;
                    let token: [u8; 32] = token.try_into().map_err(|_| {
                        rusqlite::Error::FromSqlConversionFailure(
                            0,
                            rusqlite::types::Type::Blob,
                            "workspace token has the wrong length".into(),
                        )
                    })?;
                    let joining_inviter: Option<Vec<u8>> = row.get(3)?;
                    let joining_inviter = joining_inviter
                        .map(|bytes| {
                            bytes.try_into().map_err(|_| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    3,
                                    rusqlite::types::Type::Blob,
                                    "joining inviter has the wrong length".into(),
                                )
                            })
                        })
                        .transpose()?;
                    Ok(PrivateWorkspaceSettings {
                        token: WorkspaceToken::from_bytes(token),
                        display_name: row.get(1)?,
                        relay_override: row.get(2)?,
                        joining_inviter,
                        bootstrap: row.get(4)?,
                        joining_display_name: row.get(5)?,
                        creation_creator_display_name: row.get(6)?,
                    })
                },
            )
            .optional()?
            .ok_or(WorkspaceStoreError::WorkspaceConfigurationMissing)
    }

    pub(crate) fn set_creation_creator_display_name(
        &self,
        display_name: &str,
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let changed = connection.execute(
            "UPDATE workspace_configuration SET creation_creator_display_name = ?1 WHERE singleton = 1",
            [display_name],
        )?;
        if changed == 0 {
            Err(WorkspaceStoreError::WorkspaceConfigurationMissing)
        } else {
            Ok(())
        }
    }

    pub(crate) fn set_pending_join_admission(
        &self,
        inviter: [u8; 32],
        bootstrap: &str,
        display_name: &str,
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let changed = connection.execute(
            "UPDATE workspace_configuration SET joining_inviter = ?1, bootstrap = ?2, joining_display_name = ?3 WHERE singleton = 1",
            params![inviter.as_slice(), bootstrap, display_name],
        )?;
        if changed == 0 {
            Err(WorkspaceStoreError::WorkspaceConfigurationMissing)
        } else {
            Ok(())
        }
    }

    pub(crate) fn clear_pending_join_admission(&self) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let changed = connection.execute(
            "UPDATE workspace_configuration SET joining_inviter = NULL, joining_display_name = NULL WHERE singleton = 1",
            [],
        )?;
        if changed == 0 {
            Err(WorkspaceStoreError::WorkspaceConfigurationMissing)
        } else {
            Ok(())
        }
    }

    pub(crate) fn set_lifecycle(
        &self,
        lifecycle: &WorkspaceLifecycle,
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let changed = connection.execute(
            "UPDATE workspace_configuration SET lifecycle = ?1 WHERE singleton = 1",
            [lifecycle.as_str()],
        )?;
        if changed == 0 {
            Err(WorkspaceStoreError::WorkspaceConfigurationMissing)
        } else {
            Ok(())
        }
    }

    pub fn settings(&self) -> Result<WorkspaceSettings, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection
            .query_row(
                "SELECT display_name, relay_override, lifecycle FROM workspace_configuration WHERE singleton = 1",
                [],
                |row| {
                    Ok(WorkspaceSettings {
                        display_name: row.get(0)?,
                        relay_override: row.get(1)?,
                        lifecycle: WorkspaceLifecycle::parse(&row.get::<_, String>(2)?).map_err(
                            |error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    2,
                                    rusqlite::types::Type::Text,
                                    Box::new(error),
                                )
                            },
                        )?,
                    })
                },
            )
            .optional()?
            .ok_or(WorkspaceStoreError::WorkspaceConfigurationMissing)
    }

    pub fn record_membership_operation(
        &self,
        operation_id: &str,
        signed_operation: &[u8],
    ) -> Result<(), WorkspaceStoreError> {
        validate_identifier(operation_id, "membership operation")?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "INSERT OR IGNORE INTO membership_operations (operation_id, signed_operation) VALUES (?1, ?2)",
            params![operation_id, signed_operation],
        )?;
        Ok(())
    }

    pub(crate) fn membership_operations(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection
            .prepare("SELECT signed_operation FROM membership_operations ORDER BY operation_id")?;
        let operations = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<Vec<u8>>, _>>()?;
        Ok(operations)
    }

    pub fn record_file_operation(
        &self,
        operation: &SignedFileOperation,
    ) -> Result<(), WorkspaceStoreError> {
        operation.verify()?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "INSERT OR IGNORE INTO workspace_file_operations (operation_id, signed_operation) VALUES (?1, ?2)",
            params![operation.operation.operation_id, operation.encode()?],
        )?;
        Ok(())
    }

    pub fn file_operations(&self) -> Result<Vec<SignedFileOperation>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT signed_operation FROM workspace_file_operations ORDER BY operation_id",
        )?;
        let bytes = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        bytes
            .iter()
            .map(|operation| SignedFileOperation::decode(operation).map_err(Into::into))
            .collect()
    }

    pub(crate) fn set_local_root_binding(
        &self,
        root: &Path,
        health: RootHealth,
    ) -> Result<(), WorkspaceStoreError> {
        let root = root
            .to_str()
            .ok_or(WorkspaceStoreError::InvalidIdentifier("local root path"))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO local_root_binding (singleton, root_path, health, last_error)
             VALUES (1, ?1, ?2, NULL)
             ON CONFLICT(singleton) DO UPDATE SET root_path = excluded.root_path, health = excluded.health, last_error = NULL",
            params![root, health.as_str()],
        )?;
        Ok(())
    }

    pub(crate) fn update_local_root_health(
        &self,
        health: RootHealth,
        error: Option<&str>,
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "UPDATE local_root_binding SET health = ?1, last_error = ?2 WHERE singleton = 1",
            params![health.as_str(), error],
        )?;
        Ok(())
    }

    pub fn local_root_health(&self) -> Result<Option<RootHealth>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let health = connection
            .query_row(
                "SELECT health FROM local_root_binding WHERE singleton = 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        health
            .map(|value| {
                RootHealth::parse(&value)
                    .ok_or(WorkspaceStoreError::InvalidIdentifier("local root health"))
            })
            .transpose()
    }

    pub(crate) fn replace_local_root_materialization<'a>(
        &self,
        records: impl IntoIterator<Item = &'a MaterializedRecord>,
    ) -> Result<(), WorkspaceStoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM local_root_materialization", [])?;
        for record in records {
            transaction.execute(
                "INSERT INTO local_root_materialization (node_id, relative_path, revision_id, content_hash)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    record.node_id,
                    record.relative_path,
                    record.revision_id,
                    record.content_hash
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn membership_operation_ids(&self) -> Result<Vec<String>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection
            .prepare("SELECT operation_id FROM membership_operations ORDER BY operation_id")?;
        let operation_ids = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(operation_ids)
    }

    pub fn replace_members(&self, members: &[Member]) -> Result<(), WorkspaceStoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM workspace_members", [])?;
        for member in members {
            transaction.execute(
                "INSERT INTO workspace_members (public_identity, display_name, role, added_by, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    member.public_identity,
                    member.display_name,
                    member.role,
                    member.added_by,
                    member.added_at
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn members(&self) -> Result<Vec<Member>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT public_identity, display_name, role, added_by, added_at
             FROM workspace_members ORDER BY public_identity",
        )?;
        let members = statement
            .query_map([], |row| {
                Ok(Member::new(
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<Member>, _>>()?;
        Ok(members)
    }

    /// Returns the initial shared directory recorded by the private file-history schema.
    pub fn initial_root_name(&self) -> Result<String, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection
            .query_row(
                "SELECT root_name FROM workspace_file_authority WHERE singleton = 1",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
    }
}

fn migrate(connection: &Connection) -> Result<(), WorkspaceStoreError> {
    let mut version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 && table_exists(connection, "documents")? {
        connection.execute_batch(
            "ALTER TABLE documents ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
             PRAGMA user_version = 1;",
        )?;
        version = 1;
    } else if version == 0 {
        connection.execute_batch(include_str!("../../migrations/0001_document_metadata.sql"))?;
        version = 1;
    }
    if version == 1 {
        connection.execute_batch(include_str!("../../migrations/0002_workspace_identity.sql"))?;
        version = 2;
    }
    if version == 2 {
        connection.execute_batch(include_str!(
            "../../migrations/0003_pending_join_admission.sql"
        ))?;
        version = 3;
    }
    if version == 3 {
        connection.execute_batch(include_str!(
            "../../migrations/0004_pending_join_display_name.sql"
        ))?;
        version = 4;
    }
    if version == 4 {
        connection.execute_batch(include_str!(
            "../../migrations/0005_filesystem_workspace_authority.sql"
        ))?;
        version = 5;
    }
    if version == 5 {
        connection.execute_batch(include_str!(
            "../../migrations/0006_workspace_file_history.sql"
        ))?;
        version = 6;
    }
    if version == 6 {
        connection.execute_batch(include_str!(
            "../../migrations/0007_workspace_initialization.sql"
        ))?;
        version = 7;
    }
    if version == 7 {
        connection.execute_batch(include_str!("../../migrations/0008_local_root_binding.sql"))?;
        version = 8;
    }
    if version != CURRENT_SCHEMA_VERSION {
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
