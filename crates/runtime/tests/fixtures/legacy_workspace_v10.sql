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
INSERT INTO membership_operations (operation_id, signed_operation) VALUES ('ea2d11dd985899906e3f9061a2adfcd4a186d0fec4ec0d3744e4c86327753fe1', X'01406162616261626162616261626162616261626162616261626162616261626162616261626162616261626162616261626162616261626162616261626162616200fa4834147f6e690c3693eff61336046403cd8ae2a14f31b3c4073585692395650000fa4834147f6e690c3693eff61336046403cd8ae2a14f31b3c4073585692395650341646109646576656c6f7065720240f4ebd2928331d6918e69a9b725e15e28bb3c69338dd97dbf1b3f37eedf9ba7972a616f1f369e43f4e3dcd95e09ed978ee476ed5f45377cfae85dc05efe26fa07');
INSERT INTO workspace_members (public_identity, display_name, role, added_by, added_at) VALUES ('fa4834147f6e690c3693eff61336046403cd8ae2a14f31b3c407358569239565', 'Ada', 'developer', 'fa4834147f6e690c3693eff61336046403cd8ae2a14f31b3c407358569239565', 1);
INSERT INTO workspace_file_operations (operation_id, signed_operation)
VALUES ('aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', X'010203');
PRAGMA user_version = 8;
PRAGMA foreign_keys = OFF;

ALTER TABLE workspace_configuration RENAME TO workspace_configuration_legacy;
CREATE TABLE workspace_configuration (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  token BLOB NOT NULL CHECK (length(token) = 32),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0 AND length(display_name) <= 256),
  relay_override TEXT NULL CHECK (relay_override IS NULL OR length(relay_override) <= 2048),
  lifecycle TEXT NOT NULL CHECK (lifecycle IN ('initializing', 'ready', 'joining')),
  joining_inviter BLOB NULL CHECK (joining_inviter IS NULL OR length(joining_inviter) = 32),
  bootstrap TEXT NULL CHECK (bootstrap IS NULL OR length(bootstrap) <= 1024),
  joining_display_name TEXT NULL CHECK (joining_display_name IS NULL OR (length(trim(joining_display_name)) > 0 AND length(joining_display_name) <= 256)),
  creation_creator_display_name TEXT NULL CHECK (creation_creator_display_name IS NULL OR (length(trim(creation_creator_display_name)) > 0 AND length(creation_creator_display_name) <= 256)),
  CHECK (joining_inviter IS NULL OR (joining_display_name IS NOT NULL AND bootstrap IS NOT NULL)),
  CHECK (joining_display_name IS NULL OR joining_inviter IS NOT NULL)
);
INSERT INTO workspace_configuration
  SELECT singleton, token, display_name, relay_override, lifecycle,
         joining_inviter, bootstrap, joining_display_name, creation_creator_display_name
  FROM workspace_configuration_legacy;
DROP TABLE workspace_configuration_legacy;

ALTER TABLE workspace_members RENAME TO workspace_members_legacy;
CREATE TABLE workspace_members (
  public_identity TEXT PRIMARY KEY NOT NULL
    CHECK (length(public_identity) = 64 AND public_identity NOT GLOB '*[^0-9a-f]*'),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0 AND length(display_name) <= 256),
  role TEXT NOT NULL CHECK (role IN ('viewer', 'contributor', 'developer')),
  added_by TEXT NOT NULL
    CHECK (length(added_by) = 64 AND added_by NOT GLOB '*[^0-9a-f]*'),
  added_at INTEGER NOT NULL
);
INSERT INTO workspace_members SELECT * FROM workspace_members_legacy;
DROP TABLE workspace_members_legacy;

ALTER TABLE local_root_binding RENAME TO local_root_binding_legacy;
CREATE TABLE local_root_binding (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  root_path TEXT NOT NULL,
  health TEXT NOT NULL CHECK (health IN ('healthy', 'unavailable', 'unwritable', 'unhealthy')),
  last_error TEXT NULL
);
INSERT INTO local_root_binding SELECT * FROM local_root_binding_legacy;
DROP TABLE local_root_binding_legacy;

ALTER TABLE local_root_materialization RENAME TO local_root_materialization_legacy;
CREATE TABLE local_root_materialization (
  node_id TEXT PRIMARY KEY NOT NULL,
  relative_path TEXT NOT NULL UNIQUE,
  revision_id TEXT NULL,
  content_hash TEXT NULL,
  CHECK ((revision_id IS NULL) = (content_hash IS NULL))
);
INSERT INTO local_root_materialization SELECT * FROM local_root_materialization_legacy;
DROP TABLE local_root_materialization_legacy;

PRAGMA foreign_keys = ON;
PRAGMA user_version = 9;
PRAGMA foreign_keys = OFF;

ALTER TABLE workspace_configuration RENAME TO workspace_configuration_legacy;
CREATE TABLE workspace_configuration (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  token BLOB NOT NULL CHECK (length(token) = 32),
  display_name TEXT NOT NULL CHECK (length(trim(display_name)) > 0 AND length(display_name) <= 256),
  relay_override TEXT NULL CHECK (relay_override IS NULL OR length(relay_override) <= 2048),
  lifecycle TEXT NOT NULL CHECK (lifecycle IN ('initializing', 'ready', 'joining')),
  joining_inviter BLOB NULL CHECK (joining_inviter IS NULL OR length(joining_inviter) = 32),
  bootstrap TEXT NULL CHECK (bootstrap IS NULL OR length(bootstrap) <= 1024),
  joining_display_name TEXT NULL CHECK (joining_display_name IS NULL OR (length(trim(joining_display_name)) > 0 AND length(joining_display_name) <= 256)),
  creation_creator_display_name TEXT NULL CHECK (creation_creator_display_name IS NULL OR (length(trim(creation_creator_display_name)) > 0 AND length(creation_creator_display_name) <= 256)),
  creation_stage TEXT NOT NULL DEFAULT 'store-initialized' CHECK (creation_stage IN ('store-initialized', 'creator-recorded', 'membership-initialized', 'files-initialized', 'published', 'ready')),
  CHECK (joining_inviter IS NULL OR (joining_display_name IS NOT NULL AND bootstrap IS NOT NULL)),
  CHECK (joining_display_name IS NULL OR joining_inviter IS NOT NULL)
);
INSERT INTO workspace_configuration
  SELECT singleton, token, display_name, relay_override, lifecycle,
         joining_inviter, bootstrap, joining_display_name,
         creation_creator_display_name,
         CASE WHEN lifecycle = 'initializing' THEN 'store-initialized' ELSE 'ready' END
  FROM workspace_configuration_legacy;
DROP TABLE workspace_configuration_legacy;

ALTER TABLE membership_operations RENAME TO membership_operations_legacy;
CREATE TABLE membership_operations (
  operation_id TEXT PRIMARY KEY NOT NULL
    CHECK (length(operation_id) = 64 AND operation_id NOT GLOB '*[^0-9a-f]*'),
  signed_operation BLOB NOT NULL CHECK (length(signed_operation) > 0)
);
INSERT INTO membership_operations SELECT * FROM membership_operations_legacy;
DROP TABLE membership_operations_legacy;

ALTER TABLE workspace_file_operations RENAME TO workspace_file_operations_legacy;
CREATE TABLE workspace_file_operations (
  operation_id TEXT PRIMARY KEY NOT NULL
    CHECK (length(operation_id) = 32 AND operation_id NOT GLOB '*[^0-9a-f]*'),
  signed_operation BLOB NOT NULL CHECK (length(signed_operation) > 0)
);
INSERT INTO workspace_file_operations SELECT * FROM workspace_file_operations_legacy;
DROP TABLE workspace_file_operations_legacy;

ALTER TABLE local_root_materialization RENAME TO local_root_materialization_legacy;
CREATE TABLE local_root_materialization (
  node_id TEXT PRIMARY KEY NOT NULL CHECK (length(trim(node_id)) > 0),
  relative_path TEXT NOT NULL UNIQUE CHECK (length(trim(relative_path)) > 0),
  revision_id TEXT NULL CHECK (revision_id IS NULL OR length(trim(revision_id)) > 0),
  content_hash TEXT NULL CHECK (content_hash IS NULL OR (length(content_hash) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*')),
  CHECK ((revision_id IS NULL) = (content_hash IS NULL))
);
INSERT INTO local_root_materialization SELECT * FROM local_root_materialization_legacy;
DROP TABLE local_root_materialization_legacy;

ALTER TABLE local_root_projection_journal RENAME TO local_root_projection_journal_legacy;
CREATE TABLE local_root_projection_journal (
  relative_path TEXT PRIMARY KEY NOT NULL CHECK (length(trim(relative_path)) > 0),
  temporary_name TEXT NOT NULL CHECK (length(trim(temporary_name)) > 0),
  content_hash TEXT NOT NULL CHECK (length(content_hash) = 64 AND content_hash NOT GLOB '*[^0-9a-f]*')
);
INSERT INTO local_root_projection_journal SELECT * FROM local_root_projection_journal_legacy;
DROP TABLE local_root_projection_journal_legacy;

PRAGMA foreign_keys = ON;
PRAGMA user_version = 10;
