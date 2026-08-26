CREATE TABLE IF NOT EXISTS pending_exports (
  document_id TEXT PRIMARY KEY NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_configuration (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  token BLOB NOT NULL CHECK (length(token) = 32),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0 AND length(display_name) <= 256),
  relay_override TEXT NULL CHECK (relay_override IS NULL OR length(relay_override) <= 2048),
  lifecycle TEXT NOT NULL CHECK (lifecycle IN ('initializing', 'ready', 'joining'))
);

CREATE TABLE IF NOT EXISTS membership_operations (
  operation_id TEXT PRIMARY KEY NOT NULL,
  signed_operation BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS workspace_members (
  public_identity TEXT PRIMARY KEY NOT NULL
    CHECK (length(public_identity) = 64 AND public_identity NOT GLOB '*[^0-9a-f]*'),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0 AND length(display_name) <= 256),
  role TEXT NOT NULL CHECK (role IN ('viewer', 'contributor', 'developer')),
  added_by TEXT NOT NULL
    CHECK (length(added_by) = 64 AND added_by NOT GLOB '*[^0-9a-f]*'),
  added_at INTEGER NOT NULL
);

PRAGMA user_version = 2;
