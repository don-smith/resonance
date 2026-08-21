# Resonance — Open Questions

Design uncertainties that need resolution before or during implementation. Each entry records the question, the decision point it blocks, and what would constitute an answer.

---

## OQ-01 — Iroh relay: use default or self-host?

**Blocks:** `02-system/02-transport/` spec, Phase 2 implementation

**Question:** Should the reference implementation use Iroh's default public relay (`relay.iroh.network`) or ship a self-hosted relay that teams control?

**Considerations:**
- Default relay is zero-configuration and removes infrastructure from the critical path for early development.
- Self-hosted relay is better for privacy (the relay operator can see connection metadata, though not content).
- Teams forking the runtime may want to self-host regardless of the default.
- A configurable relay URL (defaulting to the Iroh public relay) satisfies both: simple out of the box, overridable.

**Resolution path:** Decide before Phase 2. Preferred answer: configurable URL, default to Iroh public relay, document self-hosting.

---

## OQ-02 — Yjs snapshot storage format

**Blocks:** `02-system/03-documents/` spec, Phase 1 implementation

**Question:** Should Yjs document snapshots be stored as binary files alongside the Markdown file, or embedded in SQLite?

**Considerations:**
- Binary alongside the Markdown file: portable, inspectable, easy to back up, pairs naturally with the Markdown export. Requires naming convention (`<doc-id>.yjs`).
- SQLite: all persistence in one store, queryable, easier to manage transactionally. Requires a migration plan as the schema evolves.
- Yjs snapshots are binary; SQLite stores them as blobs. No semantic advantage to SQLite for binary-only data.

**Resolution path:** Decide at Phase 1 kickoff. Preferred answer: binary alongside, with a `.resonance/` directory in the workspace data folder as the container.

---

## OQ-03 — Package sandboxing level

**Blocks:** `02-system/05-packages/` spec, Phase 1 implementation

**Question:** How strictly should packages be sandboxed? Tauri provides CSP and capability-based permissions. Should the runtime enforce a strict allowlist for package capabilities, or rely on review (team packages are authored by trusted team members)?

**Considerations:**
- Strict sandboxing (capability allowlist per package) is more defensible but adds authoring friction.
- Review-based trust (packages are human-reviewed before shipping) is simpler but requires the review to actually happen.
- Member packages have a stronger sandboxing argument since they are not team-reviewed.
- The capability model should probably differ between team packages (lighter) and member packages (stricter).

**Resolution path:** Decide before Phase 1 package manifest design is finalized.

---

## OQ-04 — Update signing key management

**Blocks:** `03-delivery/` spec, Phase 1 auto-update infrastructure

**Question:** Who holds the signing key for update binaries, and what is the handover process when team ownership changes?

**Considerations:**
- The signing key is stored in CI secrets. Whoever controls CI controls updates.
- For a team-owned fork, this is typically whoever manages the repository.
- Resonance reference implementation should document a key rotation procedure.
- Losing the signing key requires a manual reinstall by all team members (the updater rejects unsigned binaries).

**Resolution path:** Document the key rotation procedure as part of Phase 1 delivery setup. The procedure does not need to be automated in v1.

---

## OQ-05 — Read-only repo content for non-developers

**Blocks:** `02-system/06-repos/` spec, Phase 5 implementation

**Question:** How do non-developer team members (who have no local repository clone) access repository content — documentation, architecture, backlog?

**Options:**
- A: Non-developers receive repo content via Iroh blob transfer from developer peers who have a local clone. No git required.
- B: Non-developers do a read-only git clone of a specific branch. Familiar, handles large repos better, but requires git knowledge and credentials for private repos.
- C: Repo packages export a read-only snapshot (a JSON blob or static HTML) that is replicated via the planning document channel. Lossy but simple.

**Resolution path:** Decide at Phase 5 kickoff. Option A is preferred for non-developer UX; Option B is the fallback if blob transfer over Iroh proves too slow for large repos.

---

## OQ-06 — Workspace scope: single or multi-team?

**Blocks:** `01-product/` requirements, Phase 2 identity design

**Question:** Does one Resonance installation support one workspace (one team) or multiple workspaces (multiple teams)?

**Considerations:**
- Single workspace: simpler identity and UI model. A consultant who works with multiple teams must run separate installations.
- Multiple workspaces: more complex (workspace switching in the shell, separate member lists, separate event buses per workspace). Required if Resonance itself is used across multiple client teams.
- Don's use case (consulting across multiple teams) suggests multi-workspace is the right long-term model.

**Resolution path:** Design the identity layer (Phase 2) to be workspace-scoped but not workspace-singular. Defer the multi-workspace UI to a post-Phase 2 follow-up.

---

## OQ-07 — Chat history replication for late-joining members

**Blocks:** `02-system/04-conversations/` spec, Phase 3 implementation

**Question:** When a new member joins a workspace, how do they receive conversation history older than their join date?

**Options:**
- A: History is replicated via Iroh blob transfer from an online peer. Complete history, requires a peer to be online at join time.
- B: History is available only from join date forward. Simple, no catch-up problem.
- C: Periodic compacted snapshots are replicated; members receive the latest snapshot plus live gossip.

**Considerations:**
- Option A provides the best new-member experience but requires a peer online at join.
- Option B is simpler and avoids the "how far back?" question.
- Teams expect chat history to be available; Option B will be perceived as a missing feature.

**Resolution path:** Implement Option A in Phase 3 with a fallback to Option B (no history) if no peer is online. Document the limitation.
