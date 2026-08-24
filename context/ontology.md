# Resonance — Ontology

Canonical terminology for the Resonance system. When a term here conflicts with an informal use elsewhere in the codebase or documentation, this definition wins.

---

## Identity and membership

**Identity.** An Ed25519 keypair held by one team member on one device. The public key is the member's identity. In normal and release builds, the private key never leaves the device's OS keychain. RFC 0008 permits only the dedicated feature-gated debug local-peer launcher to keep an owner-only checkout-local key for a validated profile; it does not expose private bytes. `See: 02-system/01-identity/`

**Workspace.** A set of members who share a workspace token. A workspace has a member list (a causally signed, deterministically projected set of public keys), a workspace file tree, and conversation channels. An installation identity may belong to multiple independently stored workspaces; the first shell presents one active workspace. `See: 02-system/01-identity/`

**Workspace token.** A 32-byte random key that identifies a workspace. A domain-separated digest derives its Iroh topic ID for workspace membership gossip. A new token invalidates access for all prior holders (the basis of v1 revocation).

**Invite token.** A short base58-encoded, inviter-signed string containing workspace data, the inviter's public key, relay configuration, and a bootstrap peer hint. Shared over any side channel. Accepting an invite requests a contributor membership through the named inviter.

**Member.** A team member whose public key appears in the workspace member list. Members may author documents and messages; updates from non-members are dropped by peers.

**Role.** A named capability level within a workspace: `viewer`, `contributor`, `developer`. Role determines which packages are visible. Role is stored in the workspace member list alongside the public key.

---

## Data domains

**Repository data.** Files managed by a Git repository. Resonance reads repository data; it never replaces Git as the sync transport. Repository data includes committed Markdown files, architecture models, and package manifests. `See: RS-R05, RS-A03`

**Workspace file.** An ordinary file or directory in the workspace file tree, never in a Git repository by virtue of workspace synchronization. Its authority is the signed file-operation history and immutable content blobs; each member may bind that tree to a different private local root. A Markdown workspace file can be opened through the rendered editor. `See: 02-system/03-documents/`

**Conversation.** An append-only log of messages organized into named channels. Messages are attributed to a member by cryptographic signature. `See: 02-system/04-conversations/`

**Channel.** A named conversation within a workspace. Channels are workspace-scoped, not repository-scoped.

---

## App and packages

**Runtime.** The Tauri-based shell that provides the app lifecycle, event bus, sync layer, identity layer, auto-update, and the consistent shared agent-panel surface. Content views remain package-owned; the runtime owns no package-specific content.

**Package.** The extensibility and implementation unit. A bundled content package contributes a TypeScript view to the app shell and interacts with the system through declared events and semantic capabilities. Packages configure the runtime-owned agent panel but do not render it. `See: 02-system/05-packages/`

**Bundled package.** A package selected from reviewed source during an app build and distributed as part of the app binary. Its source may live in an upstream checkout, a clone, or a fork. The term describes build inclusion and distribution, not repository ownership. Bundled packages win contribution conflicts.

**Member package.** A package loaded from an individual member's local configuration. Not distributed to peers. Does not affect the team's shared surface.

**Repo package.** A package that reads from a registered Git repository and emits repository events. Repo packages are loaded from the repository's package manifest (`.resonance/config.json`). `See: 02-system/06-repos/`

**Package manifest.** A JSON file declaring a package's ID, source, display name, navigation metadata, content entry, emitted and consumed events, minimum role, and optional capabilities or agent configuration. Bundled content uses `manifestVersion: 2` and `source: "bundled"`. A manifest lives at `packages/<package>/manifest.json`, or at `.resonance/config.json` once repo packages are implemented.

**Semantic capability.** A versioned SDK interface through which a package requests bounded host or Rust behavior. Capability versions evolve independently of manifest versions. A capability exposes no command names, raw transport, or private runtime state. `workspace-files:v1` is the first production capability.

**Event bus.** The Tauri event system, used as the cross-package pub/sub channel. Packages emit typed events; other packages subscribe. The runtime routes events but does not interpret semantics. `See: 02-system/05-packages/`

---

## Sync and transport

**P2P transport.** The Iroh-based layer that manages peer connections, hole-punching, relay fallback, blob replication, and gossip. Used for authenticated workspace-file recovery, conversation replication, and workspace membership. `See: 02-system/02-transport/`

**Gossip topic.** An Iroh gossip channel identified by a domain-separated digest of the workspace token. Used for membership delivery/recovery, signed presence, workspace-file history notices, and chat channel discovery. It is not the membership authority or durable file history.

**Relay.** A Resonance-operated (or team-operated) QUIC relay that forwards encrypted traffic between peers who cannot connect directly. The relay carries no plaintext content and holds no content authority. `See: RS-T01`

**Hole-punching.** A technique for establishing a direct P2P connection between two peers behind NAT. Iroh handles hole-punching; relay is the fallback.

---

## Delivery

**Update manifest.** A static JSON file hosted at a known URL. Contains the latest version number, per-platform download URLs, and cryptographic signatures. Used by `tauri-plugin-updater` to detect and deliver updates.

**Signing key.** An asymmetric key used to sign update binaries. Held by whoever controls CI for the team's fork. Used by the app to verify updates before installation. Not related to member identity keys.
