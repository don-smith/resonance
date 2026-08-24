# 0010: Use a generated bundled-package content host

Status: accepted (2026-08-24, Don Smith).

## Context

The desktop shell currently owns the complete workspace-files interface and calls Tauri commands directly. Package manifest version 1 validates one hard-coded reference manifest but has no content entry or activation lifecycle. Its `Team package` term also assumes that custom source lives in a fork, even though reviewed package source can live in any Resonance checkout.

The first content-package seam must let developers add or replace TypeScript, HTML, and CSS without editing Rust. It must keep workspace-file authority, local paths, persistence, identity, signatures, blobs, watcher state, and Iroh objects in Rust. Reviewed package modules will initially share the main webview, whose Tauri permissions apply to every module in that webview.

## Options

### Option A: Keep hard-coded package registration

Register each TypeScript entry in the desktop and each manifest in Rust. This is simple for the first package, but every added package changes host code. The registration lists can also drift, and the scaffold cannot make a package appear without manual host edits.

### Option B: Put every content package in a separate webview now

A labelled webview would provide a real Tauri permission seam. It would also add webview layout, navigation, transport, styling, and lifecycle work required for future member-loaded code rather than the reviewed bundled source in this change.

### Option C: Generate a bundled catalog and mount packages in the shared webview, chosen

Replace manifest version 1 for bundled content with manifest version 2. Version 2 uses the source term `bundled`, declares a TypeScript content entry, and names versioned semantic capabilities independently of the manifest version.

A deterministic generator scans approved package directories, validates manifests and entry paths, rejects duplicate IDs, sorts by package ID, and emits both the literal lazy imports Vite needs and the manifest catalog Rust validates. Generated outputs are checked for staleness in CI. The package scaffold regenerates the catalog, so adding a package requires no Rust source edit.

The desktop owns a package-neutral host. A package module mounts once into a host element and returns an instance with explicit activation, deactivation, and idempotent disposal. The host retains inactive instances and their elements, so tab changes do not discard drafts or package state. Mount and activation failures stay inside the package content region.

The SDK context contains only declared semantic capabilities and the declared-event interface. The workspace-files v1 capability exposes file snapshots and bounded file operations through semantic methods. Its production adapter is the only frontend package module that imports Tauri invocation and event functions. Rust maps a versioned wire request to `WorkspaceFileRuntime`, while an in-memory adapter runs package tests without Tauri.

JSON Schema remains the contract source for manifest and workspace-files wire validation. TypeScript types and Rust DTOs remain hand-written, but both sides run the same valid and invalid fixtures. Rust domain types remain private.

## Evidence

- `apps/desktop/src/main.ts` combines the runtime shell, workspace-files interface, editor ownership, and direct Tauri transport.
- `apps/desktop/src-tauri/src/commands/packages.rs` embeds one manifest and one package ID by hand.
- `packages/contracts/schema/manifest.v1.json` has no content entry and uses the fork-specific `bundled-team` source value.
- `crates/runtime/src/workspace_file_runtime.rs` already provides the deep file-authority module that the desktop adapter should call.
- `context/.decisions/0006-phase-one-runtime-foundation.md` records that one shared webview cannot isolate packages from each other.
- Focused workstream research confirmed Vite's literal glob/import requirements, Tauri's webview-level capability model, command/event behavior, and the repository's existing schema-and-fixture precedent.

## Consequences

- `Bundled package` replaces `Team package` as the canonical term. It describes build inclusion and distribution, not fork ownership. Member and repository packages remain distinct and deferred.
- Manifest version 2 replaces version 1 for bundled content. Resonance does not add a dual-version runtime loader because all currently supported manifests ship from the same source build and can migrate together.
- The shell owns package-neutral navigation, mount elements, active-package selection, and error containment. Packages own their content, state, cleanup, and first-party styles.
- The workspace-files package imports only the SDK and contract types. It cannot import desktop implementation modules or `@tauri-apps/api`.
- The Rust capability adapter sits above `WorkspaceFileRuntime`. It does not expose internal authority, store, root, blob, signing, or transport interfaces.
- Operation results and errors use a versioned, secret-free wire contract. Events only invalidate package-visible state; the adapter fetches current state through request/response operations.
- Shared-webview packages remain reviewed, trusted code. Import checks and explicit application-command permissions narrow accidental coupling but do not create hostile-code isolation.
- A future member-package loader must use separately labelled webviews. It can preserve the package lifecycle and semantic SDK while replacing the production adapter.
- Package CSS uses a package-root scope and shell-owned design tokens. Shadow DOM and separate package webviews are not part of this change.
- Package requirements, spec, authoring documentation, fixtures, and conformance tests must be updated with the implementation.
