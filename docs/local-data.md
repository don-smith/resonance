# Local workspace data

During development, Resonance bootstraps one opaque `default` workspace beneath
the platform application-data directory. It is not a workspace picker, identity,
or synchronization feature.

```text
<app-data>/.resonance/workspaces/<workspace-id>/
├── workspace.sqlite3
└── documents/
    ├── <document-id>.md
    └── <document-id>.yjs
```

`workspace.sqlite3` contains document metadata and schema migrations. Markdown
is a readable export, while the adjacent `.yjs` file contains opaque Yjs snapshot
bytes; neither replaces the other as document authority.

The runtime writes document exports through temporary files and keeps a pending
marker plus backups during replacement. Opening a workspace removes abandoned
temporary exports and restores the last complete pair if a replacement was
interrupted. Database migration is applied when the workspace opens.

This runtime data is distinct from a repository-local
`.resonance/config.json`: that future path is a repository package manifest, not
workspace state, and the runtime does not load repository packages in Phase 1.
