# 0005 — Treat repository data, planning documents, and conversations as three distinct data domains

Status: accepted (2026-08-21, Don Smith).

## Context

The workspace app handles three kinds of data that teams work with: code and files in Git repositories, planning documents, and team conversations. Each has different authorship, ownership, persistence, and sync requirements. The question is whether to model them uniformly (one sync layer for all) or as distinct domains with distinct mechanisms.

## Options

### Option A — Uniform sync layer

Treat all content as a single CRDT or event-sourced store. One sync mechanism covers everything.

Problems:
- Git repository content is already version-controlled by Git. Adding a second sync layer over it creates two sources of truth, potential conflicts between Git history and CRDT history, and confusion about which layer is authoritative.
- Conversations are append-only and message-attributed. CRDTs for conversations add unnecessary complexity; the editing model is wrong (you don't merge chat messages).
- Planning documents benefit from CRDT merge; repository files do not.
- A uniform model forces the lowest-common-denominator sync semantics onto all three domains.

### Option B — Three distinct domains — chosen

| Domain | Sync mechanism | Authoritative layer |
|--------|---------------|---------------------|
| Repository data | Git (push/pull) | Git history |
| Planning documents | Yjs CRDT over Iroh | Y.Doc (CRDT state) |
| Conversations | Append-only log over Iroh gossip | Local SQLite replica |

Each domain uses the mechanism most natural to its data shape:
- Repository content is already modeled as a DAG of commits. Git's model is correct; Resonance reads it, never replaces it.
- Planning documents are collaboratively authored, may be edited offline, and must converge. CRDTs are the correct model.
- Conversations are append-only, attributed, and temporally ordered. An append-only log with gossip replication is simpler and more correct than a CRDT for this workload.

## Evidence

- Copland's experience: using Autobase (a multi-writer append-only log) for document editing was the wrong model. Documents need CRDT merge semantics, not log linearization.
- Git is the industry-standard authority for repository content. No team tool has successfully displaced it as the source of truth; those that have tried have created confusion.
- Chat tools (Slack, Discord, Matrix) universally use append-only log models for messages, not CRDTs.

## Consequences

- Package authors must understand which domain their package operates in and use the corresponding sync primitive.
- Repo packages never write to the CRDT or conversation layer on behalf of repository content. If a repo package wants to share repository-derived content with non-developers, it does so by creating a planning document, not by replicating repository files.
- The identity layer (see 02-system/01-identity/) applies to planning documents and conversations. Git repository access is governed by Git credentials; the workspace identity layer does not mediate repository access.
- A single planning document may reference repository content (e.g., link to a commit, embed a file path) without that repository content being pulled into the planning layer.
