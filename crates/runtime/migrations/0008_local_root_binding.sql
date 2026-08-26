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

PRAGMA user_version = 8;
