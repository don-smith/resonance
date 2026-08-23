# 0009 — Use signed file operations as the workspace authority

Status: accepted (2026-08-22, Don Smith).

## Context

Planning work needs an ordinary shared file tree that members can bind to different private local roots and edit with Resonance or local tools. The earlier Yjs document model treated Markdown as an export, so it could not safely ingest external changes, carry arbitrary bytes, or preserve filesystem conflicts without introducing a second authority.

## Options

### Option A — Retain a Yjs authority with filesystem exports

A Yjs document remains authoritative and Markdown remains an export. This cannot make arbitrary external files first-class input or express binary and tree conflicts without a competing file authority.

### Option B — Use signed file operations and immutable blobs — chosen

The workspace authority is a causally ordered, member-signed file-tree operation history plus immutable BLAKE3-addressed blobs. Files and directories have stable random node IDs. Each member materializes the logical tree into one private empty root, and stable external changes become signed operations against the last materialized revision.

Disjoint Markdown edits merge deterministically from a common base. Overlaps, binary collisions, delete-versus-edit races, and tree collisions preserve each version or intent as deterministic sibling artifacts or notices until a `ResolveConflict` operation names the record. The first release retains operations, revisions, tombstones, conflicts, and blob references indefinitely.

## Evidence

- The filesystem-first workspace Scope requires a default `plans` directory, external filesystem input, byte-verified assets, different local roots, safe Markdown merge, and lossless visible conflicts.
- Filesystem synchronization research confirmed that durable operation history, private local bindings, and verified content-addressing fit offline recovery without visible control sidecars.
- The accepted design defines the portable-path, root-binding, conflict, editor, and transport boundaries.

## Consequences

- Decision 0004 is superseded for planning workspace content. Yjs snapshots, Markdown exports, document sub-topics, raw mode, and awareness carets are not part of this authority.
- Decision 0005's Git-only repository boundary remains accepted; its planning-document row becomes the workspace-file domain described here.
- Local root paths, watch state, tokens, keys, blob locations, and runtime metadata remain private and never enter the root or frontend/package contracts.
- The rendered Markdown editor reads file revisions and submits replacement intent through bounded runtime commands; it does not own replication.
- Iroh remains a transport adapter. Receivers validate signature, workspace, canonical membership, version, and limits before persisting operations or serving blobs.
