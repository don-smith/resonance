CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pending_exports (
  document_id TEXT PRIMARY KEY NOT NULL
);
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

ALTER TABLE workspace_configuration ADD COLUMN joining_inviter BLOB NULL;
ALTER TABLE workspace_configuration ADD COLUMN bootstrap TEXT NULL;
ALTER TABLE workspace_configuration ADD COLUMN joining_display_name TEXT NULL;
DROP TABLE IF EXISTS pending_exports;
DROP TABLE IF EXISTS documents;

CREATE TABLE IF NOT EXISTS workspace_file_authority (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  root_name TEXT NOT NULL CHECK (root_name = 'plans')
);

INSERT OR IGNORE INTO workspace_file_authority (singleton, root_name) VALUES (1, 'plans');

CREATE TABLE IF NOT EXISTS workspace_file_operations (
  operation_id TEXT PRIMARY KEY NOT NULL,
  signed_operation BLOB NOT NULL
);

ALTER TABLE workspace_configuration
  ADD COLUMN creation_creator_display_name TEXT NULL;

CREATE TABLE IF NOT EXISTS local_root_binding (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  root_path TEXT NOT NULL,
  health TEXT NOT NULL CHECK (health IN ('healthy', 'unavailable', 'unwritable', 'unhealthy')),
  last_error TEXT NULL
);

CREATE TABLE IF NOT EXISTS local_root_materialization (
  node_id TEXT PRIMARY KEY NOT NULL,
  relative_path TEXT NOT NULL UNIQUE,
  revision_id TEXT NULL,
  content_hash TEXT NULL,
  CHECK ((revision_id IS NULL) = (content_hash IS NULL))
);

CREATE TABLE IF NOT EXISTS local_root_projection_journal (
  relative_path TEXT PRIMARY KEY NOT NULL,
  temporary_name TEXT NOT NULL,
  content_hash TEXT NOT NULL
);

INSERT INTO workspace_configuration (singleton, token, display_name, lifecycle) VALUES (1, zeroblob(32), 'Legacy workspace', 'ready');
PRAGMA user_version = 8;
