# Local workspace data

Resonance stores its catalog and workspace data beneath the application-data root
selected at startup:

```text
<app-data>/.resonance/
├── catalog.sqlite3
└── workspaces/<workspace-id>/
    ├── workspace.sqlite3
    └── documents/
        ├── <document-id>.md
        └── <document-id>.yjs
```

`catalog.sqlite3` records the installation's workspaces and active workspace.
A workspace database contains its configuration, token, membership data, and
document metadata. Markdown is a readable export; the adjacent `.yjs` file
contains opaque Yjs snapshot bytes. Neither replaces the other as document
authority.

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
ignored directory, its key, workspace token, documents, generated profile Tauri
configuration, or any relay credentials.

The runtime writes document exports through temporary files and keeps a pending
marker plus backups during replacement. Opening a workspace removes abandoned
temporary exports and restores the last complete pair if a replacement was
interrupted. Database migration is applied when the workspace opens.

This runtime data is distinct from a repository-local `.resonance/config.json`:
that future path is a repository package manifest, not workspace state, and the
runtime does not load repository packages in Phase 1.
