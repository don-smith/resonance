ALTER TABLE workspace_configuration
  ADD COLUMN creation_creator_display_name TEXT NULL;

PRAGMA user_version = 7;
