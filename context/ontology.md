# Resonance — Ontology

Canonical terminology for the Resonance system. When a term here conflicts with an informal use elsewhere in the codebase or documentation, this definition wins.

---

## Identity and membership

**Identity.** An Ed25519 keypair held by one team member on one device. The public key is the member's identity. The private key never leaves the device's OS keychain. `See: 02-system/01-identity/`

**Workspace.** A set of members who share a workspace token. A workspace has a member list (a signed set of public keys), a set of conversation channels, and a set of planning documents. A member belongs to exactly one workspace per app installation. `See: 02-system/01-identity/`

**Workspace token.** A 32-byte random key that identifies a workspace. Used as the Iroh topic key for workspace membership gossip. A new token invalidates access for all prior holders (the basis of v1 revocation).

**Invite token.** A short encoded string containing a workspace token, the inviter's public key, and a bootstrap peer hint. Shared over any side channel. Accepting an invite joins the workspace.

**Member.** A team member whose public key appears in the workspace member list. Members may author documents and messages; updates from non-members are dropped by peers.

**Role.** A named capability level within a workspace: `viewer`, `contributor`, `developer`. Role determines which packages are visible. Role is stored in the workspace member list alongside the public key.

---

## Data domains

**Repository data.** Files managed by a Git repository. Resonance reads repository data; it never replaces Git as the sync transport. Repository data includes committed Markdown files, architecture models, and package manifests. `See: RS-R05, RS-A03`

**Planning document.** A Markdown document that lives in the workspace (not in a Git repository). Planning documents are collaboratively edited via CRDT and replicated to all workspace members. `See: 02-system/03-documents/`

**Conversation.** An append-only log of messages organized into named channels. Messages are attributed to a member by cryptographic signature. `See: 02-system/04-conversations/`

**Channel.** A named conversation within a workspace. Channels are workspace-scoped, not repository-scoped.

---

## App and packages

**Runtime.** The Tauri-based shell that provides the app lifecycle, event bus, sync layer, identity layer, auto-update, and the consistent shared agent-panel surface. Content views remain package-owned; the runtime owns no package-specific content.

**Package.** The extensibility and implementation unit. In Phase 1, Resonance validates bundled package manifests and declared events but loads no package content view. A later content package contributes a view to the app shell and interacts with the system through declared events and bounded runtime operations. Packages configure the runtime-owned agent panel but do not render it. `See: 02-system/05-packages/`

**Team package.** A package checked into the team's fork of the Resonance runtime. Distributed to all team members as part of the app binary. Team packages win contribution conflicts.

**Member package.** A package loaded from an individual member's local configuration. Not distributed to peers. Does not affect the team's shared surface.

**Repo package.** A package that reads from a registered Git repository and emits repository events. Repo packages are loaded from the repository's package manifest (`.resonance/config.json`). `See: 02-system/06-repos/`

**Package manifest.** A JSON file declaring a package's ID, source, display name, navigation metadata, events emitted, events consumed, minimum role, and optional capabilities/agent configuration. Phase 1's `manifestVersion: 1` has no webview entry point and supports bundled team manifests only. A later content-package contract may add a webview entry through an RFC. A manifest lives at `packages/<id>/manifest.json` in the runtime, or at `.resonance/config.json` in a repository once repo packages are implemented.

**Event bus.** The Tauri event system, used as the cross-package pub/sub channel. Packages emit typed events; other packages subscribe. The runtime routes events but does not interpret semantics. `See: 02-system/05-packages/`

---

## Sync and transport

**P2P transport.** The Iroh-based layer that manages peer connections, hole-punching, relay fallback, blob replication, and gossip. Used for CRDT document sync, conversation replication, and workspace membership. `See: 02-system/02-transport/`

**Gossip topic.** An Iroh gossip channel identified by a 32-byte key. Used for workspace membership, document awareness, and chat channel discovery.

**CRDT.** Conflict-free Replicated Data Type. Resonance uses Yjs as the CRDT implementation for planning documents. A CRDT guarantees eventual convergence without coordination. `See: 02-system/03-documents/`

**Relay.** A Resonance-operated (or team-operated) QUIC relay that forwards encrypted traffic between peers who cannot connect directly. The relay carries no plaintext content and holds no content authority. `See: RS-T01`

**Hole-punching.** A technique for establishing a direct P2P connection between two peers behind NAT. Iroh handles hole-punching; relay is the fallback.

---

## Delivery

**Update manifest.** A static JSON file hosted at a known URL. Contains the latest version number, per-platform download URLs, and cryptographic signatures. Used by `tauri-plugin-updater` to detect and deliver updates.

**Signing key.** An asymmetric key used to sign update binaries. Held by whoever controls CI for the team's fork. Used by the app to verify updates before installation. Not related to member identity keys.
