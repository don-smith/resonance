# 0007 — Establish workspace identity, signed membership, and peer presence

Status: accepted (2026-08-21, Don Smith).

## Context

Resonance needs its first end-to-end P2P workspace slice: one installation identity, workspace creation and invitation, a member list that converges after concurrent/offline changes, and visible peer presence. The foundation has workspace-local document storage but no identity, catalog, network runtime, or shell state. The pre-existing ontology incorrectly limits an installation member to one workspace.

The repository targets Rust 1.98.0 under the current-stable-Rust policy. Iroh 1.0.3 and Iroh Gossip 0.101.0 require Rust 1.91, and Keyring 4.1.6 requires Rust 1.88. The selected compiler supports all three and provides the required endpoint, Router, Gossip, local-relay-test, and native-keychain capabilities.

## Options

### Option A — Genesis-member authority

Make the workspace creator the sole writer of a linear member list. This simplifies conflict resolution but introduces an administrator approval path and prevents the required any-member invite flow.

### Option B — Last signed whole-list snapshot

Allow every member to publish a signed member-list snapshot and keep the most recent one. This has a small implementation but no durable causal or conflict rule: concurrent valid snapshots can overwrite different accepted membership changes.

### Option C — Causal signed membership log — chosen

Persist every authenticated, parent-linked membership operation; derive a canonical member projection by deterministic replay from a unique genesis operation. Any current member may authorize an invitation. Concurrent children of the same accepted parent resolve by lexicographic operation ID, and later evidence recomputes the projection. The complete operation set remains available for sync and diagnosis.

Accepted invites are added as `contributor`; there is no role picker or role-management UI in this slice. One installation identity may join multiple independently stored workspaces, but the shell exposes one active workspace.

## Evidence

- `.myflow/workstreams/workspace-identity-presence/research/20260821T224308Z_current-rust-and-iroh.md` confirms the Rust 1.98.0 target, Iroh 1.0.3, Iroh Gossip 0.101.0, Keyring 4.1.6, and their adapter and test seams.
- [Iroh 1.0.3 endpoint documentation](https://docs.rs/iroh/1.0.3/iroh/endpoint/struct.Endpoint.html) documents one supplied-key endpoint, relay configuration, bootstrap addresses, and async close.
- [Iroh Gossip 0.101.0 documentation](https://docs.rs/iroh-gossip/0.101.0/iroh_gossip/) documents the Iroh Router handler, topic bootstrap, and bounded overlay-neighbor events. Gossip neighbors are not a full presence directory or durable history.
- [Keyring 4.1.6 documentation](https://docs.rs/keyring/4.1.6/keyring/) documents native binary-secret storage and distinguishable missing-entry/error behavior.
- Developer decisions on 2026-08-21 selected the causal signed log and contributor default after the scope locked any-member invites and no administrator approval.

## Consequences

- The runtime keeps the installation private key only in the OS keychain at rest and exposes neither it nor workspace tokens to the frontend or packages.
- A workspace stores its token/configuration, complete membership-operation set, and materialized members independently from every other workspace. Existing foundation `default` storage is not migrated into an identity workspace.
- A single Iroh endpoint follows the active workspace relay setting. Omitted override means Iroh's public production relay map; a changed active relay requires transport restart.
- Membership sync must transmit authenticated operation sets and recover after gossip lag. Signed heartbeats provide complete known-member presence; gossip-neighbor/path observations provide direct/relayed connection detail only where available.
- The runtime event vocabulary gains `peer:connection`; the shell receives a secret-free active-workspace view and typed updates.
- The selected dependency line is deliberately pinned: Iroh 1.0.3, Iroh Gossip 0.101.0, and Keyring 4.1.6. A future change still needs a compatibility review and an explicit Rust-version decision.
