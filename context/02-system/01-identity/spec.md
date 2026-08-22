# Identity — Spec

## Status

Active.

This spec defines installation key custody, multi-workspace identity persistence, invite encoding, and deterministic membership projection. It excludes device linking, invitation expiry/enforcement, role-management UI, and workspace switching.

## 1. Installation identity

The runtime owns one Iroh Ed25519 `SecretKey` per installation. Normal and release builds read a named binary secret from the OS credential store; only a missing entry generates and writes a new 32-byte key. Locked, unavailable, malformed, ambiguous, read, and write failures are identity errors, not generation triggers. RFC 0008 permits one bounded exception: a debug Rust build with `debug-local-profiles`, started through its dedicated launcher and `--debug-profile <validated-name>` argument, may use an owner-only key under that checkout's `.resonance/debug-profiles/<name>/identity/`. The argument is rejected before custody or profile-path access in normal builds, and the feature fails compilation in release builds. The file adapter atomically publishes a first key and never replaces malformed or failed storage. The private bytes are never returned by Tauri commands/events or package interfaces and are not written to normal application storage.

The matching public key is the stable member ID and Iroh node ID. It may appear in workspace data, invites, signed envelopes, and shell read models.

## 2. Workspace persistence

A workspace ID is the lower-case hexadecimal BLAKE3 digest of the domain-separated 32-byte workspace token. A separate domain-separated BLAKE3 digest supplies the Iroh topic ID. The application catalog records workspace IDs and one active ID. Each workspace has an isolated SQLite database for workspace configuration, membership operations, and materialized membership. The configuration holds the workspace name, token, and an optional relay URL. `None` selects Iroh's public production relay mode.

An installation may have zero or more workspaces. Create and join make their workspace active. The first shell provides no workspace selection; it must not treat the foundation's legacy `default` data directory as an identity workspace.

## 3. Membership operations

The unique genesis operation is a self-signed `AddMember` for the workspace creator with role `developer`. Every later operation has exactly one parent operation ID, protocol version, workspace ID, author public key, author-local counter, operation body, and signature. The unsigned fields use the fixed v1 postcard schema and are signed with the `resonance.membership-op.v1` domain prefix. The operation ID is the BLAKE3 digest of the complete signed bytes.

A peer retains all syntactically valid signed operations. Starting at genesis, it deterministically selects the lexicographically smallest valid child of the current canonical head, applies it, and repeats. An operation is valid only when its parent is the selected head, its signer was a member in the parent projection, its workspace/version/signature are correct, and its body preserves schema invariants. New evidence recomputes the projection from genesis. Pending/missing-parent or rejected operations never grant membership.

The derived member map is keyed by public key and contains display name, role string, adding member, and advisory added time. Current clients recognize `viewer`, `contributor`, and `developer`; unknown role strings are preserved but treated no more permissively than `viewer`.

## 4. Invitations and join

An invite is base58 deterministic bytes containing protocol version, workspace ID/token/name, optional relay override, inviter public key, current Iroh `NodeAddr`, and inviter signature over the unsigned invite with the `resonance.invite.v1` domain prefix.

The joiner validates/decode the invite, stores a `joining` workspace record, registers the bootstrap address, joins the token topic, and sends a signed join request with its public key and display name. Only the named inviter, while it is canonical member, may respond with an `AddMember` operation. The added role is always `contributor`. Receipt and validation of the resulting operation completes joining. A losing concurrent branch leaves joining pending/retryable; it never grants access.

## 5. Authorization and recovery

A message from a non-member is rejected except a syntactically valid join request addressed to its current inviter and membership-sync material required to establish a joining workspace. Membership operations are individually verified before canonical replay. A full authenticated operation set is requested/rebroadcast on activation, gossip lag, and neighbor arrival so offline peers can converge without assuming gossip retained history.
