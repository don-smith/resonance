ALTER TABLE workspace_configuration ADD COLUMN joining_inviter BLOB NULL;
ALTER TABLE workspace_configuration ADD COLUMN bootstrap TEXT NULL;
PRAGMA user_version = 3;
