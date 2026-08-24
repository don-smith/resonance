# Documents — Requirements

Role: owns the workspace file tree, rendered Markdown editing, revision access, and lossless file-conflict review. The signed file-operation history and immutable content blobs are the authority; a local root is a private materialization and input surface.

---

## Requirements

### Workspace file authority

- **RS.SYS.DOC-R01 Workspace files use signed operation authority.** Directories, files, revisions, moves, tombstones, conflicts, and resolutions are represented by causally ordered, member-signed operations. Immutable BLAKE3-addressed blobs provide file bytes; neither a Yjs snapshot nor a Markdown export is authoritative. `refines: RS-R06`

- **RS.SYS.DOC-R02 Nodes have stable identities.** Every logical file and directory has a random node ID that remains stable across moves. A revision identifies its node, base revision, content reference, MIME hint, byte length, and authoring operation.

- **RS.SYS.DOC-R03 Roots are private materializations.** Each member binds one newly created or empty local root to a workspace. The runtime projects the shared tree there and ingests stable ordinary-file changes as signed revisions; absolute paths, watcher state, blobs, tokens, keys, and control metadata remain outside the root.

### Editing and conflict review

- **RS.SYS.DOC-R04 Rendered Markdown editing is the primary mode.** The editor opens a Markdown file by node ID and revision, renders it safely, and submits an authority-mediated replacement intent. It retains the member's draft when current authority advances, reports the newer revision before save, fetches that revision only for an explicit review, and replaces the draft and loaded base only after a separate explicit load action. A stale replacement remains rejected without clearing the draft. The editor does not own a hidden editor-specific replication path. `refines: RS-R06`

- **RS.SYS.DOC-R05 Raw Markdown mode and live carets are deferred.** The first filesystem workspace release provides neither raw-mode editing nor collaborative cursor awareness.

- **RS.SYS.DOC-R06 Unsafe concurrent changes remain visible.** The runtime merges only disjoint line-based Markdown changes from a common base. Overlaps, binary same-path changes, delete-versus-edit races, concurrent creates, and competing moves retain all versions or intents as deterministic sibling artifacts or notices until a member submits a resolution. Deletion or conflict updates for an open Markdown file report the authority state without clearing its local editor draft.

### Recovery and replication

- **RS.SYS.DOC-R07 File history recovery is authenticated and bounded.** Root-topic notices announce file history; authenticated streams recover missing operations and requested blobs after validating syntax, signature, workspace ID, canonical membership, protocol version, and limits. `refines: RS.SYS.TRNS-R04, RS.SYS.TRNS-R07`

- **RS.SYS.DOC-R08 Offline replicas converge without silent loss.** Receivers persist valid operations idempotently, retain unknown bases and blobs as pending, and deterministically rebuild the projection after recovery. The first release retains operations, revisions, tombstones, conflicts, and blob references indefinitely. `refines: RS-R06, RS-T04`

- **RS.SYS.DOC-R09 Generated and private data stay outside the shared input domain.** `.git`, generated conflict names, symlinks, special files, nonportable paths, and ignored paths are never accepted as ordinary input. Frontend and package contracts expose only bounded secret-free file views, health, and conflict state.
