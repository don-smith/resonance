CREATE TABLE conversation_mesh_state (
  singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
  local_address_generation INTEGER NOT NULL DEFAULT 0 CHECK (local_address_generation >= 0)
);
INSERT INTO conversation_mesh_state (singleton, local_address_generation) VALUES (1, 0);

CREATE TABLE conversation_address_notices (
  sender BLOB PRIMARY KEY NOT NULL CHECK (length(sender) = 32),
  generation INTEGER NOT NULL CHECK (generation >= 0),
  expires_at INTEGER NOT NULL,
  exact_record BLOB NOT NULL CHECK (length(exact_record) > 0)
);
