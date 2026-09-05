# Identity — Spec

## Status

Active.

This spec defines installation key custody, multi-workspace identity persistence, invite encoding, and deterministic membership projection. It excludes device linking, invitation expiry/enforcement, role-management UI, and workspace switching.

## 1. Installation identity

The runtime owns one installation Ed25519 secret per installation. Normal and release builds read a named binary secret from the OS credential store; only a missing entry generates and writes a new 32-byte key. Locked, unavailable, malformed, ambiguous, read, and write failures are identity errors, not generation triggers. RFC 0008 permits one bounded exception: a debug Rust build with `debug-local-profiles`, started through its dedicated launcher and `--debug-profile <validated-name>` argument, may use an owner-only key under that checkout's `.resonance/debug-profiles/<name>/identity/`. The argument is rejected before custody or profile-path access in normal builds, and the feature fails compilation in release builds. The file adapter atomically publishes a first key and never replaces malformed or failed storage. The private bytes are never returned by Tauri commands/events or package interfaces and are not written to normal application storage.

Iroh uses that secret directly. A crate-private identity adapter reconstructs Commonware's Ed25519 signer from the same in-memory 32 bytes and verifies that its public bytes equal `PublicIdentity`. It does not generate, persist, or expose a second signing secret. The matching public key is the stable member ID, Iroh node ID, and Commonware authenticated identity. It may appear in workspace data, invites, signed envelopes, and shell read models.

Conversations add one distinct X25519 recipient keypair per installation. Normal custody uses a separate named Keychain account; debug local profiles use a separate owner-only file. Missing-after-use, malformed, ambiguous, or inaccessible custody fails closed instead of rotating the key for the same installation identity. Only a workspace-bound, installation-signed public recipient-key record leaves the identity boundary.

## 2. Workspace persistence

A workspace ID is the lower-case hexadecimal BLAKE3 digest of the domain-separated 32-byte workspace token. A separate domain-separated BLAKE3 digest supplies the Iroh topic ID. The application catalog records workspace IDs and one active ID. Each workspace has an isolated SQLite database for workspace configuration, membership operations, and materialized membership. The configuration holds the workspace name, token, and an optional relay URL. `None` selects Iroh's public production relay mode.

An installation may have zero or more workspaces. Create and join make their workspace active. The first shell provides no workspace selection; it must not treat the foundation's legacy `default` data directory as an identity workspace.

## 3. Membership operations

The unique genesis operation is a self-signed `AddMember` for the workspace creator with role `developer`. Every later operation has exactly one parent operation ID, protocol version, workspace ID, author public key, author-local counter, operation body, and signature. The fixed postcard v1 membership body retains the existing `AddMember` bytes and appends `RemoveMember`. The operation ID is the BLAKE3 digest of complete bytes signed with the `resonance.membership-op.v1` domain prefix. Each canonical `AddMember` operation ID is the interval ID for that admission.

A removal names the target identity, current target interval, advisory time, and either `CreatorExpulsion` or an embedded `SignedSelfRemovalRequestV1`. The immutable creator is the canonical genesis author, never a mutable role. Only that creator may expel another current member or author a requested departure, and no operation may remove the creator. A departure request binds workspace, requester, current interval, genesis creator, nonce, and advisory time under its own signature domain. It is durable and idempotent but grants no membership authority by itself. The standard creator processes a valid current request automatically.

Any current member may re-admit an identity whose latest removal was a requested departure. Only the genesis creator may re-admit an identity it expelled. Re-admission opens a fresh interval, so an old request cannot remove it. Roles do not affect membership authority.

A peer retains all syntactically valid signed operations. Starting at genesis, it deterministically selects the lexicographically smallest valid child of the current canonical head, applies it, and repeats. Validation covers parent selection, signer membership, workspace, version, signature, body invariants, creator authority, interval freshness, request binding, and re-admission authority. New evidence recomputes the projection from genesis. Pending, missing-parent, losing, and rejected operations never grant membership.

For local authoring, `WorkspaceSession` returns a signed `PreparedMembershipTransition` without mutating its live log. The transition includes exact operation bytes, before/resulting heads, creator, author, interval changes, and resulting members. `WorkspaceApplication` asks `ConversationRuntime` to construct the exact resulting epoch and then uses one `WorkspaceStore` transaction for membership, epoch, materialized members, request outcome, peer-set version, and both durable publication duties. Only after commit does the session finalize projection or either transport flush. A post-commit finalization failure rebuilds from storage instead of creating another epoch.

The derived member map is keyed by public key and contains display name, role string, adding member, advisory added time, and interval provenance. Current clients recognize `viewer`, `contributor`, and `developer`; unknown role strings are preserved but treated no more permissively than `viewer`. Roles are package visibility, not admission or removal authority.

## 4. Invitations and join

An invite is base58 deterministic bytes containing protocol version, workspace ID/token/name, optional relay override, inviter public key, current Iroh `NodeAddr`, and inviter signature over the unsigned invite with the `resonance.invite.v1` domain prefix.

The joiner validates and decodes the invite, establishes its dedicated recipient-key custody, stores a `joining` workspace record, registers the bootstrap address, joins the token topic, and sends a signed join request with its public identity, display name, and conversation recipient-public-key record. Only the named inviter, while authorized for that admission, may prepare an `AddMember` operation. The added role is always `contributor`. Membership and the exact resulting conversation epoch commit atomically before publication. Receipt and validation of both authoritative records completes joining. A missing key, missing coordinator, or losing concurrent branch leaves joining pending and retryable; it never grants access.

## 5. Authorization and recovery

Current control from a non-member is rejected except a syntactically valid join request addressed to its current inviter and membership-sync material required to establish a joining workspace. Membership operations are individually verified before canonical replay. Conversation records name a canonical epoch and are authorized against the author's membership interval at that historical head, not only the current projection. A full authenticated operation set is requested and rebroadcast on activation, gossip lag, and neighbor arrival so offline peers can converge without assuming gossip retained history.
