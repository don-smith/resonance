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
