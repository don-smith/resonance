# Resonance — Open Questions

Design uncertainties that need resolution before or during implementation. Each entry records the question, the decision point it blocks, and what would constitute an answer.

---

## OQ-01 — Iroh relay: use default or self-host?

**Status:** Resolved (2026-08-21, RFC 0007).

**Decision:** Persist an optional workspace relay override. Its absence uses Iroh's public production relay mode; a configured URL creates the custom relay mode for that active workspace. The reference implementation hosts no relay and defers self-hosting documentation.

**Rationale:** This provides zero-configuration onboarding without making the public relay a permanent or compiled-in deployment decision.

---

## OQ-02 — Workspace file authority storage

**Status:** Resolved (2026-08-22, RFC 0009).

**Decision:** Keep signed file-operation history, revisions, conflict records, root bindings, watcher state, and recovery journals in private workspace SQLite. Keep immutable BLAKE3-addressed bytes in the private blob store. A local root is a disposable materialization and never holds Resonance control metadata.

**Question:** Should planning content use Yjs snapshots alongside Markdown exports, or a filesystem-first authority?

**Considerations:**
- A Yjs snapshot and Markdown export cannot ingest ordinary external files or preserve file-level conflicts without a competing authority.
- Signed file operations and immutable blobs support byte verification, deterministic recovery, and a private root binding while keeping normal files visible.

**Resolution:** RFC 0009 selected the filesystem-first authority. The superseded Yjs/export storage shape is removed without importing legacy user data because no user workspaces exist.

---

## OQ-03 — Package sandboxing level

**Status:** Resolved (2026-08-21, RFC 0006).

**Decision:** Bundled, reviewed team packages are trusted but must declare finite semantic capabilities and events. Member and repository package loaders are deferred until separate least-privilege webviews can enforce their capabilities; a shared main webview is not represented as a package sandbox.

**Question:** How strictly should packages be sandboxed? Tauri provides CSP and capability-based permissions. Should the runtime enforce a strict allowlist for package capabilities, or rely on review (team packages are authored by trusted team members)?

**Considerations:**
- Strict sandboxing (capability allowlist per package) is more defensible but adds authoring friction.
- Review-based trust (packages are human-reviewed before shipping) is simpler but requires the review to actually happen.
- Member packages have a stronger sandboxing argument since they are not team-reviewed.
- The capability model should probably differ between team packages (lighter) and member packages (stricter).

**Resolution:** RFC 0006 selected capability-declared trusted team packages and deferred untrusted loaders until enforceable webview isolation exists.

---

## OQ-04 — Update signing key management

**Status:** Resolved (2026-08-21, RFC 0006).

**Decision:** The public verification key and static-manifest endpoint are reviewed fork configuration. The private signing key is held only in CI secrets and two access-controlled recovery locations. Rotation uses an old-key-signed bridge release; loss of the old key before a bridge requires manual reinstall.

**Question:** Who holds the signing key for update binaries, and what is the handover process when team ownership changes?

**Considerations:**
- The signing key is stored in CI secrets. Whoever controls CI controls updates.
- For a team-owned fork, this is typically whoever manages the repository.
- Resonance reference implementation should document a key rotation procedure.
- Losing the signing key requires a manual reinstall by all team members (the updater rejects unsigned binaries).

**Resolution:** RFC 0006 establishes the CI/recovery custody and bridge-rotation procedure. Phase 1 supplies a fail-closed template; provisioning keys and secrets remains fork operations.

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

**Status:** Resolved (2026-08-21, RFC 0007).

**Decision:** An installation identity may join multiple independently stored workspaces. The first desktop shell exposes one active workspace and intentionally has no workspace switcher.

**Rationale:** Workspace isolation is established before collaboration data arrives, while the first UI remains small and focused.

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
