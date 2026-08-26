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
  CHECK (joining_inviter IS NULL OR (joining_display_name IS NOT NULL AND bootstrap IS NOT NULL))
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
