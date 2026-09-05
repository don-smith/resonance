//! Workspace-scoped durable file-authority storage.
//!
//! Callers provide only an application-data directory and opaque domain values.
//! SQLite, filesystem layout, migrations, and interrupted-write recovery remain
//! internal to this module.

use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection};

use crate::{
    conversations::wire::{ExactRecordV1, RecordId},
    identity::PublicIdentity,
    local_root_binding::{MaterializedRecord, RootHealth},
    membership_log::{MembershipOperationId, SignedSelfRemovalRequestV1},
    workspace_domain::{Member, WorkspaceLifecycle, WorkspaceSettings, WorkspaceToken},
    workspace_files::{
        blobs::{BlobError, WorkspaceBlobStore},
        FileOperationError, SignedFileOperation,
    },
};

const CURRENT_SCHEMA_VERSION: i32 = 11;

#[derive(Clone)]
pub(crate) struct PrivateWorkspaceSettings {
    pub token: WorkspaceToken,
    pub display_name: String,
    pub relay_override: Option<String>,
    pub joining_inviter: Option<[u8; 32]>,
    pub bootstrap: Option<String>,
    pub joining_display_name: Option<String>,
    pub creation_creator_display_name: Option<String>,
    pub creation_stage: String,
}

#[derive(Clone, Debug)]
pub struct WorkspaceStore {
    private_directory: PathBuf,
    pub(crate) connection: Arc<Mutex<Connection>>,
}

#[derive(Clone, Debug)]
pub struct AtomicMembershipEpochCommit {
    pub operation_id: MembershipOperationId,
    pub exact_membership_operation: Vec<u8>,
    pub resulting_members: Vec<Member>,
    pub previous_membership_head: Option<RecordId>,
    pub resulting_membership_head: RecordId,
    pub coordinator: [u8; 32],
    pub exact_epoch_record: Vec<u8>,
    pub epoch_record_id: RecordId,
    pub coordinator_own_envelope_opened: bool,
    pub processed_request_id: Option<MembershipOperationId>,
    pub genesis_channel: Option<ExactRecordV1>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DurablePublicationDuty {
    pub transport: String,
    pub record_id: Vec<u8>,
    pub exact_bytes: Vec<u8>,
    pub membership_head: Option<RecordId>,
}

#[derive(Debug)]
pub enum WorkspaceStoreError {
    InvalidIdentifier(&'static str),
    WorkspaceConfigurationMissing,
    UnsupportedLegacySchema,
    InitializationConflict,
    CorruptPersistedValue(&'static str),
    Io(std::io::Error),
    Database(rusqlite::Error),
    FileOperation(FileOperationError),
    FileOperationConflict,
    MembershipOperationConflict,
    ConversationRecordConflict,
    ConversationClockConflict,
    DepartureRequestConflict,
    Blob(BlobError),
    LockPoisoned,
}

impl std::fmt::Display for WorkspaceStoreError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidIdentifier(kind) => write!(formatter, "invalid {kind} identifier"),
            Self::WorkspaceConfigurationMissing => {
                formatter.write_str("workspace configuration has not been initialized")
            }
            Self::UnsupportedLegacySchema => formatter.write_str(
                "workspace schema is unsupported; export or reset this workspace before continuing",
            ),
            Self::InitializationConflict => formatter
                .write_str("workspace initialization conflicts with existing durable identity"),
            Self::CorruptPersistedValue(field) => {
                write!(formatter, "workspace storage contains an invalid {field}")
            }
            Self::Io(error) => write!(formatter, "workspace storage I/O failed: {error}"),
            Self::Database(error) => write!(formatter, "workspace database failed: {error}"),
            Self::FileOperation(error) => {
                write!(formatter, "workspace file operation failed: {error}")
            }
            Self::FileOperationConflict => {
                formatter.write_str("workspace file operation ID has conflicting durable bytes")
            }
            Self::MembershipOperationConflict => {
                formatter.write_str("membership operation ID has conflicting durable bytes")
            }
            Self::ConversationRecordConflict => {
                formatter.write_str("conversation record has conflicting durable bytes")
            }
            Self::ConversationClockConflict => {
                formatter.write_str("conversation author clock changed before atomic commit")
            }
            Self::DepartureRequestConflict => {
                formatter.write_str("departure request has conflicting durable bytes")
            }
            Self::Blob(error) => write!(formatter, "workspace blob storage failed: {error}"),
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

impl From<BlobError> for WorkspaceStoreError {
    fn from(error: BlobError) -> Self {
        Self::Blob(error)
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

        let mut connection = Connection::open(directory.join("workspace.sqlite3"))?;
        migrate(&mut connection)?;
        Ok(Self {
            private_directory: directory,
            connection: Arc::new(Mutex::new(connection)),
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
        let existing = connection
            .query_row(
                "SELECT token, display_name, relay_override, lifecycle
                 FROM workspace_configuration WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((stored_token, stored_name, stored_relay, stored_lifecycle)) = existing {
            if stored_token != token.as_bytes().as_slice()
                || stored_name != display_name
                || stored_relay.as_deref() != relay_override
                || stored_lifecycle != lifecycle.as_str()
            {
                return Err(WorkspaceStoreError::InitializationConflict);
            }
            return Ok(());
        }
        connection.execute(
            "INSERT INTO workspace_configuration (singleton, token, display_name, relay_override, lifecycle)
             VALUES (1, ?1, ?2, ?3, ?4)",
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
                "SELECT token, display_name, relay_override, joining_inviter, bootstrap, joining_display_name, creation_creator_display_name, creation_stage FROM workspace_configuration WHERE singleton = 1",
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
                        creation_stage: row.get(7)?,
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
            connection.execute(
                "UPDATE workspace_configuration SET creation_stage = 'creator-recorded' WHERE singleton = 1",
                [],
            )?;
            Ok(())
        }
    }

    pub(crate) fn set_creation_stage(&self, stage: &str) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let changed = connection.execute(
            "UPDATE workspace_configuration SET creation_stage = ?1 WHERE singleton = 1",
            [stage],
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
            "UPDATE workspace_configuration
             SET joining_inviter = NULL, joining_display_name = NULL
             WHERE singleton = 1",
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
        let row = connection
            .query_row(
                "SELECT display_name, relay_override, lifecycle FROM workspace_configuration WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((display_name, relay_override, lifecycle)) = row else {
            return Err(WorkspaceStoreError::WorkspaceConfigurationMissing);
        };
        if display_name.trim().is_empty() || display_name.len() > 256 {
            return Err(WorkspaceStoreError::CorruptPersistedValue("display name"));
        }
        let lifecycle = WorkspaceLifecycle::parse(&lifecycle)
            .map_err(|_| WorkspaceStoreError::CorruptPersistedValue("lifecycle"))?;
        Ok(WorkspaceSettings {
            display_name,
            relay_override,
            lifecycle,
        })
    }

    pub fn record_membership_operation(
        &self,
        operation_id: impl AsRef<str>,
        signed_operation: &[u8],
    ) -> Result<(), WorkspaceStoreError> {
        let operation_id = MembershipOperationId::parse(operation_id.as_ref())
            .map_err(|_| WorkspaceStoreError::InvalidIdentifier("membership operation"))?;
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO membership_operations (operation_id, signed_operation) VALUES (?1, ?2)",
            params![operation_id.as_str(), signed_operation],
        )?;
        if inserted == 0 {
            let existing = connection.query_row(
                "SELECT signed_operation FROM membership_operations WHERE operation_id = ?1",
                [operation_id.as_str()],
                |row| row.get::<_, Vec<u8>>(0),
            )?;
            if existing != signed_operation {
                return Err(WorkspaceStoreError::MembershipOperationConflict);
            }
        }
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

    pub fn commit_received_membership(
        &self,
        operation_id: &MembershipOperationId,
        exact_operation: &[u8],
        resulting_members: &[Member],
        losing_head: Option<&RecordId>,
        canonical_head_changed: bool,
    ) -> Result<(), WorkspaceStoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO membership_operations (operation_id, signed_operation) VALUES (?1, ?2)",
            params![operation_id.as_str(), exact_operation],
        )?;
        if inserted == 0 {
            let existing = transaction.query_row(
                "SELECT signed_operation FROM membership_operations WHERE operation_id = ?1",
                [operation_id.as_str()],
                |row| row.get::<_, Vec<u8>>(0),
            )?;
            if existing != exact_operation {
                return Err(WorkspaceStoreError::MembershipOperationConflict);
            }
        }
        transaction.execute("DELETE FROM workspace_members", [])?;
        for member in resulting_members {
            transaction.execute(
                "INSERT INTO workspace_members
                   (public_identity, display_name, role, added_by, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    member.public_identity.to_string(),
                    member.display_name,
                    member.role,
                    member.added_by.to_string(),
                    member.added_at
                ],
            )?;
        }
        if canonical_head_changed {
            transaction.execute(
                "UPDATE conversation_state
                 SET lookup_peer_set_version = lookup_peer_set_version + 1
                 WHERE singleton = 1",
                [],
            )?;
        }
        if let Some(head) = losing_head {
            transaction.execute(
                "UPDATE conversation_epochs SET accepted = 0 WHERE resulting_membership_head = ?1",
                [head.as_slice()],
            )?;
            transaction.execute(
                "UPDATE conversation_records SET accepted = 0 WHERE membership_head = ?1",
                [head.as_slice()],
            )?;
            transaction.execute(
                "UPDATE conversation_channel_records SET disposition = 'diagnostic'
                 WHERE authorization_epoch = ?1",
                [head.as_slice()],
            )?;
            transaction.execute(
                "UPDATE conversation_message_archive SET disposition = 'diagnostic'
                 WHERE authorization_epoch = ?1",
                [head.as_slice()],
            )?;
            transaction.execute(
                "DELETE FROM conversation_outbox WHERE record_id IN
                   (SELECT record_id FROM conversation_message_archive
                    WHERE authorization_epoch = ?1)",
                [head.as_slice()],
            )?;
            transaction.execute(
                "UPDATE publication_duties SET invalidated = 1 WHERE membership_head = ?1",
                [head.as_slice()],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn record_recipient_key(
        &self,
        exact: &ExactRecordV1,
        accepted: bool,
    ) -> Result<(), WorkspaceStoreError> {
        let crate::conversations::wire::ConversationRecordV1::RecipientKey(record) = exact.record()
        else {
            return Err(WorkspaceStoreError::CorruptPersistedValue(
                "recipient-key record family",
            ));
        };
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let inserted = connection.execute(
            "INSERT OR IGNORE INTO conversation_recipient_keys
               (record_id, installation, generation, exact_record, accepted)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                exact.id().as_slice(),
                record.installation.as_slice(),
                i64::try_from(record.generation)
                    .map_err(|_| WorkspaceStoreError::InvalidIdentifier("recipient generation"))?,
                exact.bytes(),
                accepted
            ],
        )?;
        if inserted == 0 {
            let existing = connection.query_row(
                "SELECT exact_record FROM conversation_recipient_keys WHERE record_id = ?1",
                [exact.id().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )?;
            if existing != exact.bytes() {
                return Err(WorkspaceStoreError::ConversationRecordConflict);
            }
            if accepted {
                connection.execute(
                    "UPDATE conversation_recipient_keys SET accepted = 1 WHERE record_id = ?1",
                    [exact.id().as_slice()],
                )?;
            }
        }
        Ok(())
    }

    pub fn recipient_key_records(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        self.recipient_key_records_with_filter(true)
    }

    pub(crate) fn all_recipient_key_records(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        self.recipient_key_records_with_filter(false)
    }

    fn recipient_key_records_with_filter(
        &self,
        accepted_only: bool,
    ) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let sql = if accepted_only {
            "SELECT exact_record FROM conversation_recipient_keys WHERE accepted = 1 ORDER BY installation, generation"
        } else {
            "SELECT exact_record FROM conversation_recipient_keys ORDER BY installation, generation"
        };
        let mut statement = connection.prepare(sql)?;
        let records = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<Vec<u8>>, _>>()?;
        Ok(records)
    }

    pub fn record_departure_request(
        &self,
        request: &SignedSelfRemovalRequestV1,
    ) -> Result<MembershipOperationId, WorkspaceStoreError> {
        self.record_departure_request_with_duty(request, true)
    }

    pub fn record_received_departure_request(
        &self,
        request: &SignedSelfRemovalRequestV1,
    ) -> Result<MembershipOperationId, WorkspaceStoreError> {
        self.record_departure_request_with_duty(request, false)
    }

    fn record_departure_request_with_duty(
        &self,
        request: &SignedSelfRemovalRequestV1,
        enqueue_duty: bool,
    ) -> Result<MembershipOperationId, WorkspaceStoreError> {
        request
            .verify()
            .map_err(|_| WorkspaceStoreError::CorruptPersistedValue("departure request"))?;
        let request_id = request
            .request_id()
            .map_err(|_| WorkspaceStoreError::InvalidIdentifier("departure request"))?;
        let exact = request
            .encode()
            .map_err(|_| WorkspaceStoreError::CorruptPersistedValue("departure request"))?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO departure_requests
               (request_id, requester, membership_interval_id, exact_request, status)
             VALUES (?1, ?2, ?3, ?4, 'pending')",
            params![
                request_id.as_str(),
                request.request.requester.as_slice(),
                request.request.membership_interval_id,
                exact
            ],
        )?;
        if inserted == 0 {
            let existing = transaction.query_row(
                "SELECT exact_request FROM departure_requests WHERE request_id = ?1",
                [request_id.as_str()],
                |row| row.get::<_, Vec<u8>>(0),
            )?;
            if existing != exact {
                return Err(WorkspaceStoreError::DepartureRequestConflict);
            }
        }
        if enqueue_duty {
            transaction.execute(
                "INSERT OR IGNORE INTO publication_duties
                   (transport, record_id, membership_head, exact_bytes)
                 VALUES ('iroh-departure', ?1, NULL, ?2)",
                params![request_id.as_str().as_bytes(), exact],
            )?;
        }
        transaction.commit()?;
        Ok(request_id)
    }

    pub fn pending_departure_requests(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT exact_request FROM departure_requests WHERE status = 'pending' ORDER BY request_id",
        )?;
        let requests = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<Vec<u8>>, _>>()?;
        Ok(requests)
    }

    pub fn commit_membership_epoch(
        &self,
        commit: &AtomicMembershipEpochCommit,
    ) -> Result<u64, WorkspaceStoreError> {
        if !commit.coordinator_own_envelope_opened {
            return Err(WorkspaceStoreError::CorruptPersistedValue(
                "coordinator envelope proof",
            ));
        }
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let existing_epoch = transaction
            .query_row(
                "SELECT exact_record FROM conversation_epochs WHERE resulting_membership_head = ?1",
                [commit.resulting_membership_head.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        if let Some(existing) = existing_epoch {
            if existing != commit.exact_epoch_record {
                return Err(WorkspaceStoreError::ConversationRecordConflict);
            }
            return transaction
                .query_row(
                    "SELECT lookup_peer_set_version FROM conversation_state WHERE singleton = 1",
                    [],
                    |row| row.get::<_, u64>(0),
                )
                .map_err(Into::into);
        }

        let operation_inserted = transaction.execute(
            "INSERT OR IGNORE INTO membership_operations (operation_id, signed_operation) VALUES (?1, ?2)",
            params![
                commit.operation_id.as_str(),
                commit.exact_membership_operation
            ],
        )?;
        if operation_inserted == 0 {
            let existing = transaction.query_row(
                "SELECT signed_operation FROM membership_operations WHERE operation_id = ?1",
                [commit.operation_id.as_str()],
                |row| row.get::<_, Vec<u8>>(0),
            )?;
            if existing != commit.exact_membership_operation {
                return Err(WorkspaceStoreError::MembershipOperationConflict);
            }
        }
        transaction.execute("DELETE FROM workspace_members", [])?;
        transaction.execute("UPDATE conversation_recipient_keys SET accepted = 0", [])?;
        for member in &commit.resulting_members {
            transaction.execute(
                "INSERT INTO workspace_members
                   (public_identity, display_name, role, added_by, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    member.public_identity.to_string(),
                    member.display_name,
                    member.role,
                    member.added_by.to_string(),
                    member.added_at
                ],
            )?;
            transaction.execute(
                "UPDATE conversation_recipient_keys SET accepted = 1 WHERE installation = ?1",
                [member.public_identity.as_bytes().as_slice()],
            )?;
        }
        transaction.execute(
            "INSERT INTO conversation_epochs
               (resulting_membership_head, previous_membership_head, coordinator, exact_record, own_envelope_opened)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![
                commit.resulting_membership_head.as_slice(),
                commit.previous_membership_head.as_ref().map(<[u8; 32]>::as_slice),
                commit.coordinator.as_slice(),
                commit.exact_epoch_record
            ],
        )?;
        transaction.execute(
            "INSERT INTO conversation_records (record_id, family, membership_head, exact_record)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                commit.epoch_record_id.as_slice(),
                i64::from(crate::conversations::wire::EPOCH_FAMILY),
                commit.resulting_membership_head.as_slice(),
                commit.exact_epoch_record
            ],
        )?;
        transaction.execute(
            "INSERT INTO publication_duties
               (transport, record_id, membership_head, exact_bytes)
             VALUES ('iroh-membership', ?1, ?2, ?3)",
            params![
                commit.operation_id.as_str().as_bytes(),
                commit.resulting_membership_head.as_slice(),
                commit.exact_membership_operation
            ],
        )?;
        transaction.execute(
            "INSERT INTO publication_duties
               (transport, record_id, membership_head, exact_bytes)
             VALUES ('commonware-record', ?1, ?2, ?3)",
            params![
                commit.epoch_record_id.as_slice(),
                commit.resulting_membership_head.as_slice(),
                commit.exact_epoch_record
            ],
        )?;
        if let Some(channel) = &commit.genesis_channel {
            let crate::conversations::wire::ConversationRecordV1::Channel(channel_record) =
                channel.record()
            else {
                return Err(WorkspaceStoreError::CorruptPersistedValue(
                    "genesis channel family",
                ));
            };
            transaction.execute(
                "INSERT INTO conversation_channel_records
                   (record_id, channel_id, authorization_epoch, exact_record, disposition)
                 VALUES (?1, ?2, ?3, ?4, 'accepted')",
                params![
                    channel.id().as_slice(),
                    channel_record.channel_id.as_slice(),
                    channel_record.authorization_epoch.as_slice(),
                    channel.bytes()
                ],
            )?;
            transaction.execute(
                "INSERT INTO conversation_records (record_id, family, membership_head, exact_record)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    channel.id().as_slice(),
                    i64::from(crate::conversations::wire::CHANNEL_FAMILY),
                    commit.resulting_membership_head.as_slice(),
                    channel.bytes()
                ],
            )?;
            transaction.execute(
                "INSERT INTO publication_duties
                   (transport, record_id, membership_head, exact_bytes)
                 VALUES ('commonware-record', ?1, ?2, ?3)",
                params![
                    channel.id().as_slice(),
                    commit.resulting_membership_head.as_slice(),
                    channel.bytes()
                ],
            )?;
        }
        if let Some(request_id) = &commit.processed_request_id {
            transaction.execute(
                "UPDATE departure_requests
                 SET status = 'processed', resulting_membership_head = ?1
                 WHERE request_id = ?2 AND status = 'pending'",
                params![
                    commit.resulting_membership_head.as_slice(),
                    request_id.as_str()
                ],
            )?;
        }
        transaction.execute(
            "UPDATE conversation_state
             SET lookup_peer_set_version = lookup_peer_set_version + 1
             WHERE singleton = 1",
            [],
        )?;
        let version = transaction.query_row(
            "SELECT lookup_peer_set_version FROM conversation_state WHERE singleton = 1",
            [],
            |row| row.get::<_, u64>(0),
        )?;
        transaction.commit()?;
        Ok(version)
    }

    pub fn durable_publication_duties(
        &self,
    ) -> Result<Vec<DurablePublicationDuty>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT transport, record_id, exact_bytes, membership_head
             FROM publication_duties
             WHERE delivered_at IS NULL AND invalidated = 0
             ORDER BY transport, record_id",
        )?;
        let duties = statement
            .query_map([], |row| {
                let head = row
                    .get::<_, Option<Vec<u8>>>(3)?
                    .map(|bytes| {
                        bytes.try_into().map_err(|_| {
                            rusqlite::Error::FromSqlConversionFailure(
                                3,
                                rusqlite::types::Type::Blob,
                                "membership head has the wrong length".into(),
                            )
                        })
                    })
                    .transpose()?;
                Ok(DurablePublicationDuty {
                    transport: row.get(0)?,
                    record_id: row.get(1)?,
                    exact_bytes: row.get(2)?,
                    membership_head: head,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(duties)
    }

    pub fn mark_publication_delivered(
        &self,
        transport: &str,
        record_id: &[u8],
        delivered_at: i64,
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "UPDATE publication_duties SET delivered_at = ?1
             WHERE transport = ?2 AND record_id = ?3 AND invalidated = 0",
            params![delivered_at, transport, record_id],
        )?;
        Ok(())
    }

    pub fn exact_epoch_for_head(
        &self,
        head: &RecordId,
    ) -> Result<Option<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        Ok(connection
            .query_row(
                "SELECT exact_record FROM conversation_epochs
                 WHERE resulting_membership_head = ?1 AND accepted = 1",
                [head.as_slice()],
                |row| row.get(0),
            )
            .optional()?)
    }

    pub fn lookup_peer_set_version(&self) -> Result<u64, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        Ok(connection.query_row(
            "SELECT lookup_peer_set_version FROM conversation_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?)
    }

    pub fn next_conversation_address_generation(&self) -> Result<u64, WorkspaceStoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE conversation_mesh_state
             SET local_address_generation = local_address_generation + 1
             WHERE singleton = 1",
            [],
        )?;
        let generation = transaction.query_row(
            "SELECT local_address_generation FROM conversation_mesh_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        transaction.commit()?;
        Ok(generation)
    }

    pub fn record_conversation_address_notice(
        &self,
        sender: &[u8; 32],
        generation: u64,
        expires_at: i64,
        exact_record: &[u8],
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO conversation_address_notices
               (sender, generation, expires_at, exact_record)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(sender) DO UPDATE SET
               generation = excluded.generation,
               expires_at = excluded.expires_at,
               exact_record = excluded.exact_record
             WHERE excluded.generation > conversation_address_notices.generation",
            params![sender.as_slice(), generation, expires_at, exact_record],
        )?;
        Ok(())
    }

    pub fn conversation_address_notices(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection
            .prepare("SELECT exact_record FROM conversation_address_notices ORDER BY sender")?;
        let records = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<Vec<u8>>, _>>()?;
        Ok(records)
    }

    pub fn invalidate_conversation_head(
        &self,
        losing_head: &RecordId,
    ) -> Result<(), WorkspaceStoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "UPDATE conversation_epochs SET accepted = 0 WHERE resulting_membership_head = ?1",
            [losing_head.as_slice()],
        )?;
        transaction.execute(
            "UPDATE conversation_records SET accepted = 0 WHERE membership_head = ?1",
            [losing_head.as_slice()],
        )?;
        transaction.execute(
            "UPDATE conversation_channel_records SET disposition = 'diagnostic'
             WHERE authorization_epoch = ?1",
            [losing_head.as_slice()],
        )?;
        transaction.execute(
            "UPDATE conversation_message_archive SET disposition = 'diagnostic'
             WHERE authorization_epoch = ?1",
            [losing_head.as_slice()],
        )?;
        transaction.execute(
            "DELETE FROM conversation_outbox WHERE record_id IN
               (SELECT record_id FROM conversation_message_archive
                WHERE authorization_epoch = ?1)",
            [losing_head.as_slice()],
        )?;
        transaction.execute(
            "UPDATE publication_duties SET invalidated = 1 WHERE membership_head = ?1",
            [losing_head.as_slice()],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn record_file_operation(
        &self,
        operation: &SignedFileOperation,
    ) -> Result<(), WorkspaceStoreError> {
        self.record_file_operations(std::slice::from_ref(operation))
    }

    pub fn record_file_operations(
        &self,
        operations: &[SignedFileOperation],
    ) -> Result<(), WorkspaceStoreError> {
        let encoded = operations
            .iter()
            .map(|operation| {
                operation.verify()?;
                Ok((
                    operation.operation.operation_id.clone(),
                    operation.encode()?,
                ))
            })
            .collect::<Result<Vec<_>, WorkspaceStoreError>>()?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        for (operation_id, bytes) in encoded {
            let inserted = transaction.execute(
                "INSERT OR IGNORE INTO workspace_file_operations (operation_id, signed_operation) VALUES (?1, ?2)",
                params![operation_id, bytes],
            )?;
            if inserted == 0 {
                let existing = transaction.query_row(
                    "SELECT signed_operation FROM workspace_file_operations WHERE operation_id = ?1",
                    [&operation_id],
                    |row| row.get::<_, Vec<u8>>(0),
                )?;
                if existing != bytes {
                    return Err(WorkspaceStoreError::FileOperationConflict);
                }
            }
        }
        transaction.commit()?;
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

    pub(crate) fn open_blob_store(&self) -> Result<WorkspaceBlobStore, WorkspaceStoreError> {
        Ok(WorkspaceBlobStore::open_durable(
            self.private_directory.join("blobs"),
        )?)
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
        Ok(self.local_root_binding()?.map(|(_, health)| health))
    }

    pub(crate) fn local_root_binding(
        &self,
    ) -> Result<Option<(PathBuf, RootHealth)>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let binding = connection
            .query_row(
                "SELECT root_path, health FROM local_root_binding WHERE singleton = 1",
                [],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        binding
            .map(|(path, health)| {
                let health = RootHealth::parse(&health)
                    .ok_or(WorkspaceStoreError::InvalidIdentifier("local root health"))?;
                Ok((PathBuf::from(path), health))
            })
            .transpose()
    }

    pub(crate) fn local_root_materialization(
        &self,
    ) -> Result<Vec<MaterializedRecord>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT node_id, relative_path, revision_id, content_hash
             FROM local_root_materialization ORDER BY relative_path",
        )?;
        let records = statement
            .query_map([], |row| {
                let revision_id: Option<String> = row.get(2)?;
                Ok(MaterializedRecord {
                    node_id: row.get(0)?,
                    relative_path: row.get(1)?,
                    directory: revision_id.is_none(),
                    revision_id,
                    content_hash: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(records)
    }

    pub(crate) fn clear_local_root_binding(&self) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute("DELETE FROM local_root_materialization", [])?;
        connection.execute("DELETE FROM local_root_binding", [])?;
        Ok(())
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

    pub fn typed_membership_operation_ids(
        &self,
    ) -> Result<Vec<MembershipOperationId>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection
            .prepare("SELECT operation_id FROM membership_operations ORDER BY operation_id")?;
        let result = statement
            .query_map([], |row| {
                let value: String = row.get(0)?;
                MembershipOperationId::parse(&value).map_err(|_| {
                    rusqlite::Error::FromSqlConversionFailure(
                        0,
                        rusqlite::types::Type::Text,
                        "invalid membership operation ID".into(),
                    )
                })
            })?
            .collect::<Result<Vec<_>, _>>();
        result.map_err(WorkspaceStoreError::Database)
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
                    member.public_identity.to_string(),
                    member.display_name,
                    member.role,
                    member.added_by.to_string(),
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
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(
                |(public_identity, display_name, role, added_by, added_at)| {
                    let public_identity =
                        PublicIdentity::parse(&public_identity).map_err(|_| {
                            WorkspaceStoreError::CorruptPersistedValue("member identity")
                        })?;
                    let added_by = PublicIdentity::parse(&added_by)
                        .map_err(|_| WorkspaceStoreError::CorruptPersistedValue("member author"))?;
                    if display_name.trim().is_empty() || display_name.len() > 256 {
                        return Err(WorkspaceStoreError::CorruptPersistedValue(
                            "member display name",
                        ));
                    }
                    if role.trim().is_empty() || role.len() > 64 {
                        return Err(WorkspaceStoreError::CorruptPersistedValue("member role"));
                    }
                    Ok(Member::new(
                        public_identity,
                        display_name,
                        role,
                        added_by,
                        added_at,
                    ))
                },
            )
            .collect()
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

fn migrate(connection: &mut Connection) -> Result<(), WorkspaceStoreError> {
    connection.execute_batch("BEGIN IMMEDIATE")?;
    let result = migrate_steps(connection);
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

fn migrate_steps(connection: &Connection) -> Result<(), WorkspaceStoreError> {
    let mut version: i32 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 && table_exists(connection, "documents")? {
        return Err(WorkspaceStoreError::UnsupportedLegacySchema);
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
    if version == 8 {
        connection.execute_batch(include_str!(
            "../../migrations/0009_workspace_invariants.sql"
        ))?;
        version = 9;
    }
    if version == 9 {
        connection.execute_batch(include_str!(
            "../../migrations/0010_durable_creation_and_invariants.sql"
        ))?;
        version = 10;
    }
    if version == 10 {
        connection.execute_batch(include_str!("../../migrations/0011_conversations.sql"))?;
        version = 11;
    }
    if version == 11 && !table_exists(connection, "conversation_message_archive")? {
        connection.execute_batch(include_str!("../../migrations/0011_conversation_state.sql"))?;
    }
    if version == 11 && !table_exists(connection, "conversation_mesh_state")? {
        connection.execute_batch(include_str!("../../migrations/0011_conversation_mesh.sql"))?;
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{WorkspaceStore, WorkspaceStoreError};
    use crate::workspace_domain::{WorkspaceLifecycle, WorkspaceToken};

    #[test]
    fn initialization_is_idempotent_only_for_matching_configuration() {
        let root = tempdir().expect("temporary root creates");
        let store = WorkspaceStore::open(root.path(), "workspace").expect("store opens");
        let token = WorkspaceToken::generate().expect("token generates");
        store
            .initialize_workspace(&token, "Team", None, &WorkspaceLifecycle::Ready)
            .expect("initialization succeeds");
        store
            .initialize_workspace(&token, "Team", None, &WorkspaceLifecycle::Ready)
            .expect("matching initialization is idempotent");
        let conflict = store.initialize_workspace(
            &WorkspaceToken::generate().expect("second token generates"),
            "Other",
            None,
            &WorkspaceLifecycle::Ready,
        );
        assert!(matches!(
            conflict,
            Err(WorkspaceStoreError::InitializationConflict)
        ));
    }
}
