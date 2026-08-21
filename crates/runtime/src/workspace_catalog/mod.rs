//! Catalog of opaque workspace records and the single active workspace.

use std::{
    fmt, fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{params, Connection, OptionalExtension};

use crate::{
    workspace_domain::{
        WorkspaceDomainError, WorkspaceId, WorkspaceLifecycle, WorkspaceSummary, WorkspaceToken,
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
        let connection = Connection::open(catalog_directory.join("catalog.sqlite3"))?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS workspace_catalog (
                 workspace_id TEXT PRIMARY KEY NOT NULL,
                 display_name TEXT NOT NULL,
                 lifecycle TEXT NOT NULL
             );
             CREATE TABLE IF NOT EXISTS catalog_state (
                 singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
                 active_workspace_id TEXT NULL
             );
             INSERT OR IGNORE INTO catalog_state (singleton, active_workspace_id) VALUES (1, NULL);",
        )?;
        Ok(Self {
            application_data_directory,
            connection: Mutex::new(connection),
        })
    }

    pub fn create_workspace(
        &self,
        display_name: impl Into<String>,
        relay_override: Option<String>,
    ) -> Result<WorkspaceSummary, WorkspaceCatalogError> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(WorkspaceDomainError::InvalidWorkspaceId.into());
        }
        let token = WorkspaceToken::generate()?;
        let id = token.workspace_id();
        let lifecycle = WorkspaceLifecycle::Ready;
        let store = WorkspaceStore::open(&self.application_data_directory, id.as_str())?;
        store.initialize_workspace(&token, &display_name, relay_override.as_deref(), &lifecycle)?;

        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceCatalogError::LockPoisoned)?;
        let transaction = connection.unchecked_transaction()?;
        transaction.execute(
            "INSERT INTO workspace_catalog (workspace_id, display_name, lifecycle) VALUES (?1, ?2, ?3)",
            params![id.as_str(), display_name, lifecycle.as_str()],
        )?;
        transaction.execute(
            "UPDATE catalog_state SET active_workspace_id = ?1 WHERE singleton = 1",
            [id.as_str()],
        )?;
        transaction.commit()?;
        Ok(WorkspaceSummary {
            id,
            display_name,
            lifecycle,
        })
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
        let summary = connection
            .query_row(
                "SELECT display_name, lifecycle FROM workspace_catalog WHERE workspace_id = ?1",
                [id.as_str()],
                |row| {
                    Ok(WorkspaceSummary {
                        id: id.clone(),
                        display_name: row.get(0)?,
                        lifecycle: WorkspaceLifecycle::parse(&row.get::<_, String>(1)?).map_err(
                            |error| {
                                rusqlite::Error::FromSqlConversionFailure(
                                    1,
                                    rusqlite::types::Type::Text,
                                    Box::new(error),
                                )
                            },
                        )?,
                    })
                },
            )
            .optional()?;
        Ok(summary)
    }
}
