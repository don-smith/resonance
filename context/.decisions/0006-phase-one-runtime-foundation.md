# 0006 — Establish the Phase 1 runtime foundation

Status: accepted (2026-08-21, Don Smith).

## Context

Resonance needs a runnable desktop baseline before it can safely add local
persistence, package loading, or verified update delivery. Three open questions
block that boundary: where opaque Yjs state lives, which package trust model is
actually enforceable, and how fork-owned update keys are managed. The VRS also
used `reviewer` in one identity requirement even though its ontology and product
contract use `viewer`, and package requirements incorrectly gave package-owned
HTML to the runtime-owned agent panel.

## Options

### Option A — Put snapshots in SQLite, trust all loaders, and provision delivery now

Store Markdown, metadata, and binary Yjs state in one database; permit local,
repository, and bundled packages under one review-based policy; and create a
reference endpoint and signing material during foundation work.

This centralizes storage but turns opaque bytes into database blobs, claims a
sandbox that a shared webview cannot enforce, and requires infrastructure and
secrets that are outside the reference repository's scope.

### Option B — Deep runtime seams with provisioned delivery — chosen

Keep workspace-scoped structured metadata in SQLite and adjacent Markdown/Yjs
files below the platform app-data `.resonance/` container. Load only bundled,
reviewed team manifests; require a finite declared semantic capability and event
vocabulary; defer member and repository loaders until they have separate
least-privilege webviews. The runtime owns the shared agent panel while packages
provide configuration and declared context only.

Use a static updater manifest with reviewed public configuration. CI holds the
private signing key, with two recovery custodians. A rotation is an old-key-
signed bridge release; without the old key, users must manually reinstall.
Phase 1 includes fail-closed templates and documentation but creates no key,
secret, remote, or host.

## Evidence

- Decision [0002](./0002-tauri-over-electron.md) selected Tauri for its native
  Rust runtime, capability model, and signed updater.
- Decision [0004](./0004-yjs-over-automerge.md) establishes the Y.Doc as
  authority and a binary snapshot as the sync representation.
- Decision [0005](./0005-three-data-domains.md) reserves SQLite for structured
  workspace data and keeps planning-document state distinct.
- [Tauri capabilities documentation](https://v2.tauri.app/security/capabilities/)
  scopes permissions to windows/webviews, so a shared main webview cannot be a
  per-package security boundary.
- [Tauri updater documentation](https://v2.tauri.app/plugin/updater/) supports
  signed artifacts, embedded public keys, and static JSON manifests.

## Consequences

- The initial manifest contract uses `viewer`, `contributor`, and `developer`.
  Any vocabulary change requires a package-contract RFC.
- `agentPanel` HTML entries and Tauri-command names are not manifest API.
  Agent permissions are finite semantic operations enforced by the runtime.
- The `WorkspaceStore` interface must hide paths, SQL, migrations, atomic
  replacement, and recovery from its callers.
- Development remains secret-free. Release validation fails closed until a fork
  provides a non-placeholder public key, HTTPS endpoint, and CI-held signing
  secret.
- No identity, Iroh transport, collaboration, content package, repository
  loader, member loader, conversation, or agent execution ships in this phase.
