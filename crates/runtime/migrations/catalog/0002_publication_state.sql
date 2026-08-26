ALTER TABLE workspace_catalog
  ADD COLUMN publication_state TEXT NOT NULL DEFAULT 'published'
  CHECK (publication_state IN ('published'));

PRAGMA user_version = 2;
