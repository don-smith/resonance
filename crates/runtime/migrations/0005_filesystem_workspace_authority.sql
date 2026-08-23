DROP TABLE IF EXISTS pending_exports;
DROP TABLE IF EXISTS documents;

CREATE TABLE IF NOT EXISTS workspace_file_authority (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  root_name TEXT NOT NULL CHECK (root_name = 'plans')
);

INSERT OR IGNORE INTO workspace_file_authority (singleton, root_name) VALUES (1, 'plans');

PRAGMA user_version = 5;
