CREATE TABLE IF NOT EXISTS workspace_file_operations (
  operation_id TEXT PRIMARY KEY NOT NULL,
  signed_operation BLOB NOT NULL
);

PRAGMA user_version = 6;
