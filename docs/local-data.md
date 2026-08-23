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
membership data, signed file-operation history, node and revision records,
conflicts, ignore rules, root bindings, watcher state, and recovery journals.
`blobs/` contains immutable BLAKE3-addressed file bytes. These locations are
private runtime state: they are not the shared workspace root and no package or
frontend command receives their paths.

The shared workspace authority is the signed operation history plus verified
blobs. A member binds a newly created or empty local directory as a private
materialization of that authority. Its visible root begins with `plans` and
contains ordinary workspace files only. Resonance never writes tokens, private
keys, SQLite data, blob-store files, watcher markers, or other control metadata
there. Git-managed content remains a separate Git-only domain.

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
└── app-data/.resonance/
    ├── catalog.sqlite3
    └── workspaces/
```

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
