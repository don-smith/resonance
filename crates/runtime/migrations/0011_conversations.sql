ALTER TABLE workspace_members RENAME TO workspace_members_v10;
CREATE TABLE workspace_members (
  public_identity TEXT PRIMARY KEY NOT NULL
    CHECK (length(public_identity) = 64 AND public_identity NOT GLOB '*[^0-9a-f]*'),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0 AND length(display_name) <= 256),
  role TEXT NOT NULL CHECK (length(trim(role)) > 0 AND length(role) <= 64),
  added_by TEXT NOT NULL
    CHECK (length(added_by) = 64 AND added_by NOT GLOB '*[^0-9a-f]*'),
  added_at INTEGER NOT NULL
);
INSERT INTO workspace_members SELECT * FROM workspace_members_v10;
DROP TABLE workspace_members_v10;

CREATE TABLE conversation_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  lookup_peer_set_version INTEGER NOT NULL DEFAULT 0 CHECK (lookup_peer_set_version >= 0)
);
INSERT INTO conversation_state (singleton, lookup_peer_set_version) VALUES (1, 0);

CREATE TABLE conversation_recipient_keys (
  record_id BLOB PRIMARY KEY NOT NULL CHECK (length(record_id) = 32),
  installation BLOB NOT NULL CHECK (length(installation) = 32),
  generation INTEGER NOT NULL CHECK (generation >= 0),
  exact_record BLOB NOT NULL CHECK (length(exact_record) > 0),
  accepted INTEGER NOT NULL CHECK (accepted IN (0, 1)),
  UNIQUE (installation, generation)
);

CREATE TABLE conversation_epochs (
  resulting_membership_head BLOB PRIMARY KEY NOT NULL CHECK (length(resulting_membership_head) = 32),
  previous_membership_head BLOB NULL CHECK (previous_membership_head IS NULL OR length(previous_membership_head) = 32),
  coordinator BLOB NOT NULL CHECK (length(coordinator) = 32),
  exact_record BLOB NOT NULL CHECK (length(exact_record) > 0),
  own_envelope_opened INTEGER NOT NULL CHECK (own_envelope_opened = 1),
  accepted INTEGER NOT NULL DEFAULT 1 CHECK (accepted IN (0, 1))
);

CREATE TABLE conversation_records (
  record_id BLOB PRIMARY KEY NOT NULL CHECK (length(record_id) = 32),
  family INTEGER NOT NULL CHECK (family BETWEEN 1 AND 8),
  membership_head BLOB NOT NULL CHECK (length(membership_head) = 32),
  exact_record BLOB NOT NULL CHECK (length(exact_record) > 0),
  accepted INTEGER NOT NULL DEFAULT 1 CHECK (accepted IN (0, 1))
);

CREATE TABLE departure_requests (
  request_id TEXT PRIMARY KEY NOT NULL
    CHECK (length(request_id) = 64 AND request_id NOT GLOB '*[^0-9a-f]*'),
  requester BLOB NOT NULL CHECK (length(requester) = 32),
  membership_interval_id TEXT NOT NULL
    CHECK (length(membership_interval_id) = 64 AND membership_interval_id NOT GLOB '*[^0-9a-f]*'),
  exact_request BLOB NOT NULL CHECK (length(exact_request) > 0),
  status TEXT NOT NULL CHECK (status IN ('pending', 'processed', 'invalidated')),
  resulting_membership_head BLOB NULL CHECK (resulting_membership_head IS NULL OR length(resulting_membership_head) = 32)
);

CREATE TABLE publication_duties (
  transport TEXT NOT NULL CHECK (transport IN ('iroh-membership', 'iroh-departure', 'commonware-record')),
  record_id BLOB NOT NULL CHECK (length(record_id) IN (32, 64)),
  membership_head BLOB NULL CHECK (membership_head IS NULL OR length(membership_head) = 32),
  exact_bytes BLOB NOT NULL CHECK (length(exact_bytes) > 0),
  invalidated INTEGER NOT NULL DEFAULT 0 CHECK (invalidated IN (0, 1)),
  delivered_at INTEGER NULL,
  PRIMARY KEY (transport, record_id)
);

PRAGMA user_version = 11;
