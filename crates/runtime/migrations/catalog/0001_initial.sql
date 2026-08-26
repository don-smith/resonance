CREATE TABLE IF NOT EXISTS workspace_catalog (
  workspace_id TEXT PRIMARY KEY NOT NULL
    CHECK (length(workspace_id) = 64 AND workspace_id NOT GLOB '*[^0-9a-f]*')
);

CREATE TABLE IF NOT EXISTS catalog_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  active_workspace_id TEXT NULL
);

INSERT OR IGNORE INTO catalog_state (singleton, active_workspace_id) VALUES (1, NULL);

PRAGMA user_version = 1;
