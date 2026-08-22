# 0008 — Bound file-backed identities to debug local peer profiles

Status: accepted (2026-08-22, Don Smith).

## Context

RFC 0007 requires native credential-store custody for the production installation identity. Development also needs a way for one checkout to demonstrate the real Iroh and Gossip collaboration path with two persistent local desktop peers. Sharing one identity or normal application-data root would not demonstrate peer membership or persistence isolation.

## Decision

Normal and release Resonance builds continue to use only the native credential store. They reject `--debug-profile` during startup argument parsing, before app-data lookup, profile-root construction, or custody construction.

A Rust debug build compiled with the `debug-local-profiles` feature may accept exactly one validated `--debug-profile <name>` application argument. The feature is a compile error in non-debug Rust builds. `pnpm desktop:profiles -- <name-a> <name-b>` is the supported development launcher; it starts two names with separate Vite ports, Tauri configuration, and macOS development bundles. `pnpm desktop:profiles -- --reset <name>` is the only reset path.

Each name maps only to the direct child `.resonance/debug-profiles/<name>/` in the current checkout. The root holds an advisory lifetime lock, an owner-only file-backed 32-byte installation key, and a private `app-data/` root. The file adapter publishes a first key through a flushed same-directory temporary file and atomic link, then fails closed for malformed, inaccessible, or failed storage. It never falls back to native storage or regenerates a replacement key after a non-missing failure.

The launcher and macOS runner use a name only to coordinate processes and bundles. Names, paths, raw key bytes, and a file-custody switch do not enter Tauri commands, events, packages, UI state, or runtime public read models. RFC 0007 remains the production custody decision.

## Evidence

- The existing `InstallationIdentity` / `KeyCustody` boundary accepts a second adapter without exposing private bytes.
- `WorkspaceCatalog::open` already isolates all catalog and workspace storage from one supplied application-data root.
- Debug-feature runtime and desktop tests cover stable owner-only file keys, malformed-key failure, name validation, symlink refusal, and same-profile locking; normal startup tests cover the pre-custody argument gate.
- The launcher contract assigns deterministic distinct ports, URLs, identifiers, and feature/application arguments to the two profiles.

## Consequences

- File-backed private keys are an explicit, checkout-local development exception and are ignored by Git; they are not an import, export, migration, or production fallback mechanism.
- The initial launcher targets the existing macOS development signing path. Unsupported file-permission semantics fail closed.
- A developer must use the profile launcher—not `desktop:dev`—to run two independent local peers. Reset refuses an active or unverifiable profile lock and removes only an inactive validated child.
