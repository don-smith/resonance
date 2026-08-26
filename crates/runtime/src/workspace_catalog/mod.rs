//! Catalog of opaque workspace records and the single active workspace.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{Connection, OptionalExtension};

use crate::{
    workspace_domain::{
        display_name as validate_display_name, WorkspaceDomainError, WorkspaceId,
        WorkspaceLifecycle, WorkspaceSummary, WorkspaceToken,
    },
    workspace_store::{WorkspaceStore, WorkspaceStoreError},
};

#[derive(Debug)]
pub enum WorkspaceCatalogError {
    Domain(WorkspaceDomainError),
    Store(WorkspaceStoreError),
    Database(rusqlite::Error),
    Io(std::io::Error),
    LockPoisoned,
    UnsupportedSchema,
    UnknownWorkspace,
}

impl fmt::Display for WorkspaceCatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Domain(error) => write!(formatter, "workspace domain error: {error}"),
            Self::Store(error) => write!(formatter, "workspace storage error: {error}"),
            Self::Database(error) => {
                write!(formatter, "workspace catalog database failed: {error}")
            }
            Self::Io(error) => write!(formatter, "workspace catalog I/O failed: {error}"),
            Self::LockPoisoned => formatter.write_str("workspace catalog lock was poisoned"),
            Self::UnsupportedSchema => formatter.write_str(
                "workspace catalog schema is unsupported; export or reset the catalog before continuing",
            ),
            Self::UnknownWorkspace => {
                formatter.write_str("workspace is not in this installation catalog")
            }
        }
    }
}

impl std::error::Error for WorkspaceCatalogError {}

impl From<WorkspaceDomainError> for WorkspaceCatalogError {
    fn from(error: WorkspaceDomainError) -> Self {
        Self::Domain(error)
    }
}

impl From<WorkspaceStoreError> for WorkspaceCatalogError {
    fn from(error: WorkspaceStoreError) -> Self {
        Self::Store(error)
    }
}

impl From<rusqlite::Error> for WorkspaceCatalogError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Database(error)
    }
}

impl From<std::io::Error> for WorkspaceCatalogError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

pub struct WorkspaceCatalog {
    application_data_directory: PathBuf,
    connection: Mutex<Connection>,
}

impl WorkspaceCatalog {
    pub fn open(
        application_data_directory: impl AsRef<Path>,
    ) -> Result<Self, WorkspaceCatalogError> {
        let application_data_directory = application_data_directory.as_ref().to_path_buf();
        let catalog_directory = application_data_directory.join(".resonance");
        fs::create_dir_all(&catalog_directory)?;
        let mut connection = Connection::open(catalog_directory.join("catalog.sqlite3"))?;
        migrate_catalog(&mut connection)?;
        let catalog = Self {
            application_data_directory,
            connection: Mutex::new(connection),
        };
        catalog.recover_workspace_references()?;
        Ok(catalog)
    }

    fn recover_workspace_references(&self) -> Result<(), WorkspaceCatalogError> {
        let workspaces_directory = self
            .application_data_directory
            .join(".resonance")
            .join("workspaces");
        let Ok(entries) = fs::read_dir(workspaces_directory) else {
            return Ok(());
        };
        for entry in entries {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let Some(workspace_id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Ok(workspace_id) = WorkspaceId::parse(&workspace_id) else {
                continue;
            };
            let store =
                WorkspaceStore::open(&self.application_data_directory, workspace_id.as_str())?;
            match store.settings() {
                Ok(_) => {}
                Err(WorkspaceStoreError::WorkspaceConfigurationMissing) => continue,
                Err(error) => return Err(error.into()),
            }
            let connection = self
                .connection
                .lock()
                .map_err(|_| WorkspaceCatalogError::LockPoisoned)?;
            connection.execute(
                "INSERT OR IGNORE INTO workspace_catalog (workspace_id) VALUES (?1)",
                [workspace_id.as_str()],
            )?;
            connection.execute(
                "UPDATE catalog_state SET active_workspace_id = ?1
                 WHERE singleton = 1 AND active_workspace_id IS NULL",
                [workspace_id.as_str()],
            )?;
        }
        Ok(())
    }

    pub fn create_workspace(
        &self,
        display_name: impl Into<String>,
        relay_override: Option<String>,
    ) -> Result<WorkspaceSummary, WorkspaceCatalogError> {
        let display_name = validate_display_name(display_name)?;
        self.create_workspace_with_token(
            WorkspaceToken::generate()?,
            display_name,
            relay_override,
            WorkspaceLifecycle::Ready,
        )
    }

    pub(crate) fn create_workspace_with_token(
        &self,
        token: WorkspaceToken,
        display_name: String,
        relay_override: Option<String>,
        lifecycle: WorkspaceLifecycle,
    ) -> Result<WorkspaceSummary, WorkspaceCatalogError> {
        let display_name = validate_display_name(display_name)?;
        let id = token.workspace_id();
        let store = WorkspaceStore::open(&self.application_data_directory, id.as_str())?;
        store.initialize_workspace(&token, &display_name, relay_override.as_deref(), &lifecycle)?;
        if lifecycle == WorkspaceLifecycle::Initializing {
            return Ok(WorkspaceSummary {
                id,
                display_name,
                lifecycle,
            });
        }
        if lifecycle == WorkspaceLifecycle::Ready {
            store.set_creation_stage("files-initialized")?;
            self.publish_workspace(&id)?;
        } else {
            self.publish_workspace_reference(&id)?;
        }
        Ok(WorkspaceSummary {
            id,
            display_name,
            lifecycle,
        })
    }

    pub(crate) fn publish_workspace(&self, id: &WorkspaceId) -> Result<(), WorkspaceCatalogError> {
        let store = WorkspaceStore::open(&self.application_data_directory, id.as_str())?;
        let settings = store.settings()?;
        if settings.lifecycle != WorkspaceLifecycle::Ready {
            return Err(WorkspaceCatalogError::Store(
                WorkspaceStoreError::InitializationConflict,
            ));
        }
        self.publish_workspace_reference(id)?;
        store.set_creation_stage("published")?;
        Ok(())
    }

    fn publish_workspace_reference(&self, id: &WorkspaceId) -> Result<(), WorkspaceCatalogError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceCatalogError::LockPoisoned)?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT OR IGNORE INTO workspace_catalog (workspace_id) VALUES (?1)",
            [id.as_str()],
        )?;
        transaction.execute(
            "UPDATE catalog_state SET active_workspace_id = ?1 WHERE singleton = 1",
            [id.as_str()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn open_workspace_for_initialization(
        &self,
        id: &WorkspaceId,
    ) -> Result<WorkspaceStore, WorkspaceCatalogError> {
        Ok(WorkspaceStore::open(
            &self.application_data_directory,
            id.as_str(),
        )?)
    }

    pub(crate) fn set_workspace_lifecycle(
        &self,
        id: &WorkspaceId,
        lifecycle: WorkspaceLifecycle,
    ) -> Result<(), WorkspaceCatalogError> {
        let store = self.open_workspace(id)?;
        store.set_lifecycle(&lifecycle)?;
        // WorkspaceStore is the sole durable owner of mutable metadata.
        Ok(())
    }

    pub fn open_workspace(
        &self,
        id: &WorkspaceId,
    ) -> Result<WorkspaceStore, WorkspaceCatalogError> {
        if self.workspace_summary(id)?.is_none() {
            return Err(WorkspaceCatalogError::UnknownWorkspace);
        }
        Ok(WorkspaceStore::open(
            &self.application_data_directory,
            id.as_str(),
        )?)
    }

    pub fn set_active_workspace(&self, id: &WorkspaceId) -> Result<(), WorkspaceCatalogError> {
        if self.workspace_summary(id)?.is_none() {
            return Err(WorkspaceCatalogError::UnknownWorkspace);
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceCatalogError::LockPoisoned)?;
        connection.execute(
            "UPDATE catalog_state SET active_workspace_id = ?1 WHERE singleton = 1",
            [id.as_str()],
        )?;
        Ok(())
    }

    pub fn active_workspace(&self) -> Result<Option<WorkspaceSummary>, WorkspaceCatalogError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceCatalogError::LockPoisoned)?;
        let id = connection.query_row(
            "SELECT active_workspace_id FROM catalog_state WHERE singleton = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )?;
        drop(connection);
        match id {
            Some(id) => self.workspace_summary(&WorkspaceId::parse(&id)?),
            None => Ok(None),
        }
    }

    fn workspace_summary(
        &self,
        id: &WorkspaceId,
    ) -> Result<Option<WorkspaceSummary>, WorkspaceCatalogError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceCatalogError::LockPoisoned)?;
        let exists = connection
            .query_row(
                "SELECT 1 FROM workspace_catalog WHERE workspace_id = ?1",
                [id.as_str()],
                |_| Ok(()),
            )
            .optional()?;
        drop(connection);
        exists
            .map(|()| {
                let store = WorkspaceStore::open(&self.application_data_directory, id.as_str())?;
                let settings = store.settings()?;
                Ok(WorkspaceSummary {
                    id: id.clone(),
                    display_name: settings.display_name,
                    lifecycle: settings.lifecycle,
                })
            })
            .transpose()
    }
}

fn migrate_catalog(connection: &mut Connection) -> Result<(), WorkspaceCatalogError> {
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = migrate_catalog_steps(connection);
    match result {
        Ok(()) => {
            connection.execute_batch("COMMIT")?;
            Ok(())
        }
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK");
            Err(error)
        }
    }
}

fn migrate_catalog_steps(connection: &Connection) -> Result<(), WorkspaceCatalogError> {
    let version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    match version {
        0 => {
            if table_has_column(connection, "workspace_catalog", "display_name")? {
                connection.execute_batch(
                    "ALTER TABLE workspace_catalog RENAME TO workspace_catalog_legacy;
                     CREATE TABLE workspace_catalog (
                       workspace_id TEXT PRIMARY KEY NOT NULL
                         CHECK (length(workspace_id) = 64 AND workspace_id NOT GLOB '*[^0-9a-f]*')
                     );
                     INSERT INTO workspace_catalog (workspace_id)
                       SELECT workspace_id FROM workspace_catalog_legacy;
                     DROP TABLE workspace_catalog_legacy;",
                )?;
            }
            connection.execute_batch(include_str!("../../migrations/catalog/0001_initial.sql"))?;
        }
        1 | 2 => {}
        _ => return Err(WorkspaceCatalogError::UnsupportedSchema),
    }
    let version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 1 {
        connection.execute_batch(include_str!(
            "../../migrations/catalog/0002_publication_state.sql"
        ))?;
    }
    let version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != 2 {
        return Err(WorkspaceCatalogError::UnsupportedSchema);
    }
    Ok(())
}

fn table_has_column(
    connection: &Connection,
    table: &str,
    column: &str,
) -> Result<bool, WorkspaceCatalogError> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(columns.iter().any(|name| name == column))
}
