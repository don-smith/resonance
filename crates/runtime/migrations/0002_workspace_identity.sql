CREATE TABLE IF NOT EXISTS pending_exports (
  document_id TEXT PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_configuration (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  token BLOB NOT NULL CHECK (length(token) = 32),
  display_name TEXT NOT NULL,
  relay_override TEXT NULL,
  lifecycle TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS membership_operations (
  operation_id TEXT PRIMARY KEY NOT NULL,
  signed_operation BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_members (
  public_identity TEXT PRIMARY KEY NOT NULL,
  display_name TEXT NOT NULL,
  role TEXT NOT NULL,
  added_by TEXT NOT NULL,
  added_at INTEGER NOT NULL
);

PRAGMA user_version = 2;
