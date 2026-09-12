CREATE TABLE conversation_channel_records (
  record_id BLOB PRIMARY KEY NOT NULL CHECK (length(record_id) = 32),
  channel_id BLOB NOT NULL CHECK (length(channel_id) = 16),
  authorization_epoch BLOB NOT NULL CHECK (length(authorization_epoch) = 32),
  exact_record BLOB NOT NULL CHECK (length(exact_record) > 0),
  disposition TEXT NOT NULL CHECK (disposition IN ('accepted', 'diagnostic'))
);

CREATE TABLE conversation_message_archive (
  record_id BLOB PRIMARY KEY NOT NULL CHECK (length(record_id) = 32),
  channel_id BLOB NOT NULL CHECK (length(channel_id) = 16),
  authorization_epoch BLOB NOT NULL CHECK (length(authorization_epoch) = 32),
  channel_head BLOB NOT NULL CHECK (length(channel_head) = 32),
  author BLOB NOT NULL CHECK (length(author) = 32),
  author_sequence INTEGER NOT NULL CHECK (author_sequence >= 0),
  lamport INTEGER NOT NULL CHECK (lamport >= 0),
  created_at INTEGER NOT NULL,
  exact_record BLOB NOT NULL CHECK (length(exact_record) > 0),
  disposition TEXT NOT NULL CHECK (disposition IN ('accepted', 'quarantined', 'diagnostic'))
);
CREATE INDEX conversation_message_author_sequence
  ON conversation_message_archive(author, author_sequence);
CREATE INDEX conversation_message_order
  ON conversation_message_archive(channel_id, lamport, author, author_sequence, record_id);

CREATE TABLE conversation_outbox (
  record_id BLOB PRIMARY KEY NOT NULL CHECK (length(record_id) = 32),
  exact_record BLOB NOT NULL CHECK (length(exact_record) > 0),
  created_at INTEGER NOT NULL,
  FOREIGN KEY (record_id) REFERENCES conversation_message_archive(record_id) ON DELETE CASCADE
);

CREATE TABLE conversation_local_clock (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  next_author_sequence INTEGER NOT NULL DEFAULT 0 CHECK (next_author_sequence >= 0),
  lamport INTEGER NOT NULL DEFAULT 0 CHECK (lamport >= 0)
);
INSERT INTO conversation_local_clock (singleton, next_author_sequence, lamport) VALUES (1, 0, 0);

CREATE TABLE conversation_read_positions (
  channel_id BLOB PRIMARY KEY NOT NULL CHECK (length(channel_id) = 16),
  record_id BLOB NOT NULL CHECK (length(record_id) = 32),
  lamport INTEGER NOT NULL CHECK (lamport >= 0),
  author BLOB NOT NULL CHECK (length(author) = 32),
  author_sequence INTEGER NOT NULL CHECK (author_sequence >= 0)
);

CREATE TABLE conversation_diagnostics (
  sequence INTEGER PRIMARY KEY AUTOINCREMENT,
  record_id BLOB NULL CHECK (record_id IS NULL OR length(record_id) = 32),
  exact_bytes BLOB NOT NULL CHECK (length(exact_bytes) > 0),
  reason TEXT NOT NULL CHECK (length(reason) > 0 AND length(reason) <= 128)
);
