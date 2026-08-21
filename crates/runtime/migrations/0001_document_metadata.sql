CREATE TABLE IF NOT EXISTS documents (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL,
  updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS pending_exports (
  document_id TEXT PRIMARY KEY NOT NULL
);
PRAGMA user_version = 1;
