CREATE TABLE documents (
  id TEXT PRIMARY KEY NOT NULL,
  title TEXT NOT NULL
);
INSERT INTO documents (id, title) VALUES ('legacy-doc', 'Legacy document');
PRAGMA user_version = 0;
