# 0004 — Use Yjs for document sync, not Automerge

Status: accepted (2026-08-21, Don Smith).

## Context

Planning documents require real-time collaborative editing that converges offline without data loss. Two CRDT libraries are candidates: Automerge (used in Copland) and Yjs. The document editor is TipTap (a ProseMirror-based rich text editor). The transport is Iroh.

## Options

### Option A — Automerge

Automerge is a general-purpose CRDT that handles arbitrary JSON-shaped data. Copland used it for Markdown document sync.

Problems:
- Automerge's editor integration story is weaker. There is no first-class TipTap or ProseMirror binding; integration requires custom adapters.
- Automerge was used alongside Autobase in Copland. With Autobase removed (see decision 0003), Automerge would be the only CRDT layer, which is appropriate — but its text-editing performance is less optimized than Yjs for the collaborative editing workload.
- The Automerge Wasm bundle is larger than Yjs and startup is slower on first load.

### Option B — Yjs — chosen

Yjs is a CRDT designed specifically for collaborative text editing. It supports arbitrary data types (`Y.Map`, `Y.Array`, `Y.Text`) but its text primitives are highly optimized for the editing workload.

Advantages:
- TipTap has a first-class Yjs extension (`@tiptap/extension-collaboration`). The integration is maintained by the TipTap team and is production-grade.
- CodeMirror 6 also has a Yjs binding, enabling the raw Markdown editing mode to share the same `Y.Doc` as the rich text editor.
- Yjs is transport-agnostic. A thin adapter sends `Y.Doc` update messages over an Iroh QUIC stream. No coupling between the CRDT layer and the transport layer.
- Yjs is used in production at scale (HackMD, linear.app, Loom, and others). The risk profile is lower than Automerge for the collaborative text use case.

Tradeoffs accepted:
- Yjs's internal merge algorithm is an implementation detail; debugging merge anomalies requires understanding Yjs internals, which are not intuitive.
- Yjs's data model is not JSON-first (unlike Automerge), which is slightly less natural for structured metadata. For planning documents (predominantly text), this is not a meaningful limitation.

## Evidence

- Copland's Automerge usage showed correct convergence behavior but slow sync performance. The source of slowness was not fully isolated (Autobase, Automerge, or Hyperswarm), making Automerge's contribution unclear. Yjs's better editor integration removes one variable.
- TipTap's collaboration extension is documented and maintained; Automerge would require a custom adapter.

## Consequences

- Copland's Automerge integration is not reused. Yjs is the CRDT layer for all planning documents.
- The `Y.Doc` is the authoritative in-memory representation. The Markdown file on disk is an export of the document; the Yjs binary is the sync-layer representation.
- A `y-iroh` transport adapter must be written (thin: sends Yjs update messages as Iroh blob/gossip payloads).
- Automerge remains available for consideration if a structured-metadata use case emerges that Yjs handles poorly.
