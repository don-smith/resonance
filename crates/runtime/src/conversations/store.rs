//! SQLite persistence helpers for exact socket-free conversation state.

use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, OptionalExtension as _};

use crate::workspace_store::{WorkspaceStore, WorkspaceStoreError};

type MessageOrder = (u64, [u8; 32], u64, [u8; 32]);
type SparseGapMap = BTreeMap<[u8; 32], Vec<(u64, u64)>>;

use super::{
    channels::MAX_DIAGNOSTIC_RECORDS,
    wire::{ChannelId, ExactRecordV1, MessageRecordV1, RecordId},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MessageDisposition {
    Accepted,
    Quarantined,
    Diagnostic,
}

impl MessageDisposition {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Quarantined => "quarantined",
            Self::Diagnostic => "diagnostic",
        }
    }

    fn parse(value: &str) -> Result<Self, WorkspaceStoreError> {
        match value {
            "accepted" => Ok(Self::Accepted),
            "quarantined" => Ok(Self::Quarantined),
            "diagnostic" => Ok(Self::Diagnostic),
            _ => Err(WorkspaceStoreError::CorruptPersistedValue(
                "message disposition",
            )),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct StoredMessage {
    pub exact: ExactRecordV1,
    pub disposition: MessageDisposition,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageCommitOutcome {
    Accepted,
    Duplicate,
    Equivocation,
}

impl WorkspaceStore {
    pub(crate) fn conversation_epoch_records(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT exact_record FROM conversation_epochs WHERE accepted = 1 ORDER BY resulting_membership_head",
        )?;
        let rows = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<Vec<u8>>, _>>()?;
        Ok(rows)
    }

    pub(crate) fn record_received_epoch(
        &self,
        exact: &ExactRecordV1,
    ) -> Result<(), WorkspaceStoreError> {
        let super::wire::ConversationRecordV1::Epoch(record) = exact.record() else {
            return Err(WorkspaceStoreError::CorruptPersistedValue(
                "epoch record family",
            ));
        };
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let inserted = transaction.execute(
            "INSERT OR IGNORE INTO conversation_epochs
               (resulting_membership_head, previous_membership_head, coordinator,
                exact_record, own_envelope_opened)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![
                record.resulting_membership_head.as_slice(),
                record
                    .previous_membership_head
                    .as_ref()
                    .map(<[u8; 32]>::as_slice),
                record.coordinator.as_slice(),
                exact.bytes()
            ],
        )?;
        if inserted == 0 {
            let existing = transaction.query_row(
                "SELECT exact_record FROM conversation_epochs WHERE resulting_membership_head = ?1",
                [record.resulting_membership_head.as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )?;
            if existing != exact.bytes() {
                return Err(WorkspaceStoreError::ConversationRecordConflict);
            }
        }
        transaction.execute(
            "INSERT OR IGNORE INTO conversation_records
               (record_id, family, membership_head, exact_record)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                exact.id().as_slice(),
                i64::from(super::wire::EPOCH_FAMILY),
                record.resulting_membership_head.as_slice(),
                exact.bytes()
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn conversation_channel_records(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection
            .prepare("SELECT exact_record FROM conversation_channel_records ORDER BY record_id")?;
        let rows = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<Vec<u8>>, _>>()?;
        Ok(rows)
    }

    pub(crate) fn record_channel_records(
        &self,
        records: &[ExactRecordV1],
        accepted: &BTreeSet<RecordId>,
        outbound: Option<&ExactRecordV1>,
    ) -> Result<(), WorkspaceStoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        for exact in records {
            let super::wire::ConversationRecordV1::Channel(record) = exact.record() else {
                return Err(WorkspaceStoreError::CorruptPersistedValue(
                    "channel record family",
                ));
            };
            let disposition = if accepted.contains(exact.id()) {
                "accepted"
            } else {
                "diagnostic"
            };
            let inserted = transaction.execute(
                "INSERT OR IGNORE INTO conversation_channel_records
                   (record_id, channel_id, authorization_epoch, exact_record, disposition)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    exact.id().as_slice(),
                    record.channel_id.as_slice(),
                    record.authorization_epoch.as_slice(),
                    exact.bytes(),
                    disposition
                ],
            )?;
            if inserted == 0 {
                let existing = transaction.query_row(
                    "SELECT exact_record FROM conversation_channel_records WHERE record_id = ?1",
                    [exact.id().as_slice()],
                    |row| row.get::<_, Vec<u8>>(0),
                )?;
                if existing != exact.bytes() {
                    return Err(WorkspaceStoreError::ConversationRecordConflict);
                }
                transaction.execute(
                    "UPDATE conversation_channel_records SET disposition = ?1 WHERE record_id = ?2",
                    params![disposition, exact.id().as_slice()],
                )?;
            }
            transaction.execute(
                "INSERT OR IGNORE INTO conversation_records
                   (record_id, family, membership_head, exact_record, accepted)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    exact.id().as_slice(),
                    i64::from(super::wire::CHANNEL_FAMILY),
                    record.authorization_epoch.as_slice(),
                    exact.bytes(),
                    accepted.contains(exact.id())
                ],
            )?;
            transaction.execute(
                "UPDATE conversation_records SET accepted = ?1 WHERE record_id = ?2",
                params![accepted.contains(exact.id()), exact.id().as_slice()],
            )?;
            transaction.execute(
                "UPDATE publication_duties SET invalidated = ?1 WHERE record_id = ?2",
                params![!accepted.contains(exact.id()), exact.id().as_slice()],
            )?;
        }
        if let Some(exact) = outbound.filter(|exact| accepted.contains(exact.id())) {
            let super::wire::ConversationRecordV1::Channel(record) = exact.record() else {
                return Err(WorkspaceStoreError::CorruptPersistedValue(
                    "outbound channel family",
                ));
            };
            transaction.execute(
                "INSERT OR IGNORE INTO publication_duties
                   (transport, record_id, membership_head, exact_bytes)
                 VALUES ('commonware-record', ?1, ?2, ?3)",
                params![
                    exact.id().as_slice(),
                    record.authorization_epoch.as_slice(),
                    exact.bytes()
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn local_conversation_clock(&self) -> Result<(u64, u64), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        Ok(connection.query_row(
            "SELECT next_author_sequence, lamport FROM conversation_local_clock WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?)
    }

    pub(crate) fn commit_local_message(
        &self,
        exact: &ExactRecordV1,
        record: &MessageRecordV1,
        expected_sequence: u64,
        expected_lamport: u64,
    ) -> Result<(), WorkspaceStoreError> {
        let sequence = to_sql_integer(record.author_sequence, "message sequence")?;
        let lamport = to_sql_integer(record.lamport, "message Lamport value")?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        let clock = transaction.query_row(
            "SELECT next_author_sequence, lamport FROM conversation_local_clock WHERE singleton = 1",
            [],
            |row| Ok((row.get::<_, u64>(0)?, row.get::<_, u64>(1)?)),
        )?;
        if clock != (expected_sequence, expected_lamport)
            || record.author_sequence != expected_sequence
            || record.lamport != expected_lamport.saturating_add(1)
        {
            return Err(WorkspaceStoreError::ConversationClockConflict);
        }
        insert_message_row(&transaction, exact, record, MessageDisposition::Accepted)?;
        transaction.execute(
            "INSERT INTO conversation_outbox (record_id, exact_record, created_at)
             VALUES (?1, ?2, ?3)",
            params![exact.id().as_slice(), exact.bytes(), record.created_at],
        )?;
        transaction.execute(
            "INSERT INTO publication_duties
               (transport, record_id, membership_head, exact_bytes)
             VALUES ('commonware-record', ?1, ?2, ?3)",
            params![
                exact.id().as_slice(),
                record.authorization_epoch.as_slice(),
                exact.bytes()
            ],
        )?;
        transaction.execute(
            "UPDATE conversation_local_clock
             SET next_author_sequence = ?1, lamport = ?2 WHERE singleton = 1",
            params![sequence.saturating_add(1), lamport],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn commit_received_message(
        &self,
        exact: &ExactRecordV1,
        record: &MessageRecordV1,
    ) -> Result<MessageCommitOutcome, WorkspaceStoreError> {
        to_sql_integer(record.author_sequence, "message sequence")?;
        let lamport = to_sql_integer(record.lamport, "message Lamport value")?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        if let Some(existing) = transaction
            .query_row(
                "SELECT exact_record FROM conversation_message_archive WHERE record_id = ?1",
                [exact.id().as_slice()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
        {
            if existing != exact.bytes() {
                return Err(WorkspaceStoreError::ConversationRecordConflict);
            }
            return Ok(MessageCommitOutcome::Duplicate);
        }
        let competing = transaction
            .query_row(
                "SELECT record_id FROM conversation_message_archive
                 WHERE author = ?1 AND author_sequence = ?2 LIMIT 1",
                params![record.author.as_slice(), record.author_sequence],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        let disposition = if competing.is_some() {
            MessageDisposition::Quarantined
        } else {
            MessageDisposition::Accepted
        };
        insert_message_row(&transaction, exact, record, disposition)?;
        if competing.is_some() {
            transaction.execute(
                "UPDATE conversation_message_archive SET disposition = 'quarantined'
                 WHERE author = ?1 AND author_sequence = ?2",
                params![record.author.as_slice(), record.author_sequence],
            )?;
            transaction.execute(
                "UPDATE conversation_records SET accepted = 0
                 WHERE record_id IN
                   (SELECT record_id FROM conversation_message_archive
                    WHERE author = ?1 AND author_sequence = ?2)",
                params![record.author.as_slice(), record.author_sequence],
            )?;
            transaction.execute(
                "DELETE FROM conversation_outbox WHERE record_id IN
                   (SELECT record_id FROM conversation_message_archive
                    WHERE author = ?1 AND author_sequence = ?2)",
                params![record.author.as_slice(), record.author_sequence],
            )?;
            transaction.execute(
                "UPDATE publication_duties SET invalidated = 1
                 WHERE record_id IN
                   (SELECT record_id FROM conversation_message_archive
                    WHERE author = ?1 AND author_sequence = ?2)",
                params![record.author.as_slice(), record.author_sequence],
            )?;
        }
        transaction.execute(
            "UPDATE conversation_local_clock SET lamport = MAX(lamport, ?1) WHERE singleton = 1",
            [lamport],
        )?;
        transaction.commit()?;
        Ok(if competing.is_some() {
            MessageCommitOutcome::Equivocation
        } else {
            MessageCommitOutcome::Accepted
        })
    }

    pub(crate) fn stored_messages(&self) -> Result<Vec<StoredMessage>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT exact_record, disposition FROM conversation_message_archive
             ORDER BY lamport, author, author_sequence, record_id",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(bytes, disposition)| {
                Ok(StoredMessage {
                    exact: ExactRecordV1::decode(&bytes).map_err(|_| {
                        WorkspaceStoreError::CorruptPersistedValue("conversation message")
                    })?,
                    disposition: MessageDisposition::parse(&disposition)?,
                })
            })
            .collect()
    }

    pub(crate) fn conversation_outbox(&self) -> Result<Vec<Vec<u8>>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT exact_record FROM conversation_outbox ORDER BY created_at, record_id",
        )?;
        let rows = statement
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<Vec<u8>>, _>>()?;
        Ok(rows)
    }

    pub(crate) fn mark_conversation_read(
        &self,
        channel_id: &ChannelId,
        record: &MessageRecordV1,
        record_id: &RecordId,
    ) -> Result<(), WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        connection.execute(
            "INSERT INTO conversation_read_positions
               (channel_id, record_id, lamport, author, author_sequence)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(channel_id) DO UPDATE SET
               record_id = excluded.record_id,
               lamport = excluded.lamport,
               author = excluded.author,
               author_sequence = excluded.author_sequence
             WHERE (excluded.lamport, excluded.author, excluded.author_sequence, excluded.record_id)
                 > (lamport, author, author_sequence, record_id)",
            params![
                channel_id.as_slice(),
                record_id.as_slice(),
                record.lamport,
                record.author.as_slice(),
                record.author_sequence
            ],
        )?;
        Ok(())
    }

    pub(crate) fn conversation_read_position(
        &self,
        channel_id: &ChannelId,
    ) -> Result<Option<MessageOrder>, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        Ok(connection
            .query_row(
                "SELECT lamport, author, author_sequence, record_id
                 FROM conversation_read_positions WHERE channel_id = ?1",
                [channel_id.as_slice()],
                |row| {
                    let author: Vec<u8> = row.get(1)?;
                    let record_id: Vec<u8> = row.get(3)?;
                    Ok((
                        row.get(0)?,
                        author
                            .try_into()
                            .map_err(|_| conversion_error(1, "read author"))?,
                        row.get(2)?,
                        record_id
                            .try_into()
                            .map_err(|_| conversion_error(3, "read record ID"))?,
                    ))
                },
            )
            .optional()?)
    }

    pub(crate) fn sparse_recovery_gaps(
        &self,
        maximum_ranges: usize,
    ) -> Result<SparseGapMap, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let mut statement = connection.prepare(
            "SELECT author, author_sequence FROM conversation_message_archive
             ORDER BY author, author_sequence",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, u64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut present = BTreeMap::<[u8; 32], BTreeSet<u64>>::new();
        for (author, sequence) in rows {
            let author = author
                .try_into()
                .map_err(|_| WorkspaceStoreError::CorruptPersistedValue("message author"))?;
            present.entry(author).or_default().insert(sequence);
        }
        let mut output = BTreeMap::new();
        let mut remaining_ranges = maximum_ranges;
        for (author, sequences) in present {
            if remaining_ranges == 0 {
                break;
            }
            let Some(highest) = sequences.last().copied() else {
                continue;
            };
            let mut gaps = Vec::new();
            let mut cursor = 0;
            while cursor < highest && gaps.len() < remaining_ranges {
                if sequences.contains(&cursor) {
                    cursor += 1;
                    continue;
                }
                let first = cursor;
                while cursor < highest && !sequences.contains(&cursor) {
                    cursor += 1;
                }
                gaps.push((first, cursor - 1));
            }
            remaining_ranges = remaining_ranges.saturating_sub(gaps.len());
            output.insert(author, gaps);
        }
        Ok(output)
    }

    pub(crate) fn record_conversation_diagnostic(
        &self,
        bytes: &[u8],
        record_id: Option<&RecordId>,
        reason: &str,
    ) -> Result<(), WorkspaceStoreError> {
        if reason.len() > 128 {
            return Err(WorkspaceStoreError::CorruptPersistedValue(
                "conversation diagnostic reason",
            ));
        }
        let bytes = &bytes[..bytes.len().min(super::wire::MAX_RECORD_BYTES)];
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO conversation_diagnostics (record_id, exact_bytes, reason)
             VALUES (?1, ?2, ?3)",
            params![record_id.map(<[u8; 32]>::as_slice), bytes, reason],
        )?;
        transaction.execute(
            "DELETE FROM conversation_diagnostics WHERE sequence NOT IN
               (SELECT sequence FROM conversation_diagnostics ORDER BY sequence DESC LIMIT ?1)",
            [MAX_DIAGNOSTIC_RECORDS],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub(crate) fn conversation_diagnostic_count(&self) -> Result<usize, WorkspaceStoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| WorkspaceStoreError::LockPoisoned)?;
        Ok(
            connection.query_row("SELECT COUNT(*) FROM conversation_diagnostics", [], |row| {
                row.get(0)
            })?,
        )
    }
}

fn insert_message_row(
    transaction: &rusqlite::Transaction<'_>,
    exact: &ExactRecordV1,
    record: &MessageRecordV1,
    disposition: MessageDisposition,
) -> Result<(), WorkspaceStoreError> {
    transaction.execute(
        "INSERT INTO conversation_message_archive
           (record_id, channel_id, authorization_epoch, channel_head, author,
            author_sequence, lamport, created_at, exact_record, disposition)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            exact.id().as_slice(),
            record.channel_id.as_slice(),
            record.authorization_epoch.as_slice(),
            record.channel_head.as_slice(),
            record.author.as_slice(),
            record.author_sequence,
            record.lamport,
            record.created_at,
            exact.bytes(),
            disposition.as_str()
        ],
    )?;
    transaction.execute(
        "INSERT OR IGNORE INTO conversation_records
           (record_id, family, membership_head, exact_record)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            exact.id().as_slice(),
            i64::from(super::wire::MESSAGE_FAMILY),
            record.authorization_epoch.as_slice(),
            exact.bytes()
        ],
    )?;
    Ok(())
}

fn to_sql_integer(value: u64, field: &'static str) -> Result<i64, WorkspaceStoreError> {
    i64::try_from(value).map_err(|_| WorkspaceStoreError::CorruptPersistedValue(field))
}

fn conversion_error(index: usize, field: &'static str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Blob,
        format!("{field} has the wrong length").into(),
    )
}
