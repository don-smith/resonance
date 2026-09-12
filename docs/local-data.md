# Local workspace data

Resonance stores its catalog and private workspace authority beneath the
application-data root selected at startup:

```text
<app-data>/.resonance/
├── catalog.sqlite3
└── workspaces/<workspace-id>/
    ├── workspace.sqlite3
    └── blobs/
        └── <blake3-content-hash>
```

`catalog.sqlite3` records the installation's workspaces and active workspace.
A workspace database stores private configuration, the workspace token,
membership data, signed file-operation history, encrypted conversation archives
and outboxes, durable Iroh and Commonware publication duties, device-local unread
positions, the local root binding, and the last materialized node and revision
for each local path. Workspace databases use schema v11 for the conversation
membership, epoch, archive, recovery, and direct-address state. The runtime rebuilds
logical nodes, revisions, tombstones, conflicts, and active ignore rules from
the signed operation history when it opens the workspace. `blobs/` contains
immutable BLAKE3-addressed file bytes. Conversation archive and outbox rows keep
exact signed ciphertext records; they do not store message Markdown or epoch
secrets in plaintext. These locations are private runtime
state. They are not the shared workspace root, and no package or frontend
command receives their paths.

The shared workspace authority is the signed operation history plus verified
blobs. A member binds a newly created or empty local directory as a private
materialization of that authority. Its visible root begins with `plans` and
contains ordinary workspace files only. Resonance never writes tokens, private
keys, SQLite data, blob-store files, watcher markers, or other control metadata
there. Git-managed content remains a separate Git-only domain.

Ignore rules are replicated signed operations, not files in the shared root.
Patterns are anchored to the workspace root and use `/` between segments. `*`
matches within one segment, `?` matches one character, and a complete `**`
segment matches any number of path segments. Bracket classes, braces, absolute
paths, dot segments, non-NFC text, and partial `**` segments are rejected. A
new rule cannot hide a live logical node. `.git` and generated
`.resonance-conflict-` names are permanently excluded and cannot be configured.
The projector and filesystem scanner use the same active rule set.

Ordinary `pnpm desktop:dev` uses Tauri's platform application-data directory
and native Keychain custody. On macOS, its development identifier normally
places that data under
`~/Library/Application Support/com.resonance.desktop/`. This is the only
normal or release identity custody path.

For the macOS-only local-peer demonstration, `pnpm desktop:profiles -- alice
bob` starts two feature-gated debug applications. Each profile has an ignored,
owner-only checkout-local root:

```text
.resonance/debug-profiles/<name>/
├── identity/installation.key
├── identity/conversation-recipient.key
├── identity/conversation-recipient.created
└── app-data/.resonance/
    ├── catalog.sqlite3
    └── workspaces/
```

The installation signing key and dedicated X25519 conversation-recipient key
use separate owner-only files in a debug profile. Ordinary installations keep
the same separation in distinct native Keychain accounts. Missing or malformed
recipient custody fails closed rather than silently rotating after use.

Profile keys and state never share normal app data or native credentials. They
are a development-only exception defined by RFC 0008, not a production file-key
fallback. Reset one inactive profile with `pnpm desktop:profiles -- --reset
<name>`; it removes only that validated profile root. Do not commit this
ignored directory, its key, workspace token, generated profile Tauri
configuration, or any relay credentials.

The runtime applies database migrations when a workspace opens. It persists a
valid operation and its byte reference before materializing a bound root;
interrupted projection writes are repaired from private state on restart.
Unavailable or unwritable roots are reported as unhealthy without blocking
history recovery.

This runtime data is distinct from a repository-local `.resonance/config.json`:
that future path is a repository package manifest, not workspace state, and the
runtime does not load repository packages in Phase 1.
