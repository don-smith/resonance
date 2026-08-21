# Documents — Requirements

Role: owns planning document creation, CRDT sync via Yjs, the rendered Markdown editor (TipTap), raw Markdown mode, local persistence, and the transport adapter (y-iroh).

---

## Requirements

### Document model

- **RS.SYS.DOC-R01 Each document is a Yjs Y.Doc.** The `Y.Doc` is the authoritative in-memory representation. The Markdown file is an export. The Yjs binary snapshot is the sync-layer representation. `refines: RS-R06`

- **RS.SYS.DOC-R02 Documents are identified by a UUID.** The document ID is stable across renames. Document metadata (title, last-edited-by, last-edited-at) is stored in the workspace SQLite database, not in the document itself.

- **RS.SYS.DOC-R03 Documents are persisted locally as Markdown files.** On every significant edit (debounced), the document is serialized to Markdown and written to the workspace data directory. A Yjs binary snapshot is written alongside it. `refines: RS-R06`

### Editing

- **RS.SYS.DOC-R04 Rendered Markdown editing is the primary mode.** TipTap renders documents as rendered Markdown — headers, bold, italic, blockquotes, lists, and links are displayed in their final form while remaining editable in place. Formatting is expressed through TipTap marks and nodes, serialized to Markdown by `tiptap-markdown`. This is not a traditional rich text editor (no font pickers, font sizes, or text colors); it is a WYSIWYG Markdown editing experience. `refines: RS-R06`

- **RS.SYS.DOC-R05 Raw Markdown mode is a toggle.** A raw Markdown editor (CodeMirror 6) is available as a toggle. It shares the same `Y.Text` node as the TipTap editor, so both views converge on the same document. The toggle is per-window; two peers can use different modes simultaneously without conflict.

- **RS.SYS.DOC-R06 Collaborative cursors are shown.** When two or more peers have the same document open, each peer's cursor and selection is shown in the other's view, attributed by display name and color. Implemented via Yjs awareness protocol.

### Sync

- **RS.SYS.DOC-R07 Document sync uses a per-document Iroh gossip sub-topic.** The sub-topic key is derived from the document ID. Yjs update messages are sent as gossip payloads. `refines: RS.SYS.TRNS-R04`

- **RS.SYS.DOC-R08 Offline edits converge on reconnection.** Yjs handles merge of concurrent offline edits without data loss. The runtime sends accumulated updates on reconnection. `refines: RS-R06, RS-T04`

- **RS.SYS.DOC-R09 New peers receive a document snapshot on join.** When a peer joins a document's gossip topic, it requests the current Yjs state vector from an online peer. The online peer responds with the missing updates (or a full snapshot if the joining peer has no state). `refines: RS.SYS.TRNS-R07`

---

## Open Design Questions

- **RS.SYS.DOC-DQ01** Should documents support embedded images? **Position: yes, deferred from v1.** Images are linked Markdown assets (`![alt](path/to/asset)`) stored in a workspace assets directory (e.g., `docs/assets/`) alongside the Markdown files. The assets directory is synced as Iroh blobs in tandem with the document. The v1 UX is expected to be minimal — the editor renders inline images from Markdown links, and users place assets in the directory manually. Drag-and-drop, paste handling, and an asset picker are deferred beyond v1.

- **RS.SYS.DOC-DQ02** Should the document list be flat (all documents in one list) or hierarchical (folders)? Flat is simpler; hierarchical matches how teams think about planning documents. A flat list with tags is a middle path.

- **RS.SYS.DOC-DQ03** Should the editor support diagramming extensions (e.g., Mermaid)? **Position: yes, deferred from v1.** Mermaid diagrams are inline fenced code blocks with a `mermaid` language tag, rendered client-side by a TipTap node extension. The expectation is that a defined set of supported rendered-code-block extensions (Mermaid, mathematical notation, etc.) will be installed and available. The specific extension set and configuration mechanism are deferred beyond v1.
