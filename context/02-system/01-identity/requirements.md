# Identity — Requirements

Role: owns keypair generation and storage, workspace creation, invite token generation and acceptance, member list management, and access control enforcement.

---

## Assumptions

- **RS.SYS.ID-A01 One identity per installation.** A member has one Ed25519 keypair per machine. One installation identity may join multiple independently stored workspaces. Multiple devices require multiple identities (device linking is a future concern).

- **RS.SYS.ID-A02 Workspace token is the shared secret.** Knowledge of the workspace token grants the ability to attempt to join the workspace. The member list is the second gate.

---

## Requirements

### Keypair management

- **RS.SYS.ID-R01 Keypair generated on first launch.** Normal and release builds generate a keypair if and only if no keypair exists in the OS keychain, before any workspace interaction. Locked, unavailable, malformed, ambiguous, read, and write failures block identity actions; the private key is never persisted outside the keychain or exposed through frontend/package APIs. The only exception is RFC 0008's debug-only `debug-local-profiles` build and dedicated launcher, which persist an owner-only checkout-local key for a validated named local peer and remain unreachable from normal or release builds.

- **RS.SYS.ID-R02 Public key is the stable identity.** The public key is used as the member's ID in all signed artifacts (conversation records, workspace-file operations, member-list entries). Iroh and Commonware authentication reconstruct their signers from the same in-memory 32-byte installation secret and must produce the same public bytes. No transport persists a second installation signing secret. Display names are advisory and may change; the public key does not.

- **RS.SYS.ID-R13 Conversations use separate recipient-key custody.** Each installation owns one dedicated X25519 recipient keypair for HPKE epoch envelopes. It uses a distinct native Keychain account and debug-profile file from the installation Ed25519 key. Missing-after-use, malformed, ambiguous, or inaccessible custody fails closed and never silently rotates that recipient key for the same installation identity. Only the signed public record may leave the identity boundary. `refines: RS.SYS.ID-R01`

### Workspace

- **RS.SYS.ID-R03 Workspace creation generates a random token.** The workspace token is 32 bytes of cryptographically random data. A domain-separated digest of it supplies both the Iroh gossip topic ID and opaque local workspace ID. `refines: RS-R02`

- **RS.SYS.ID-R04 The workspace member list is a causally signed set.** The derived member map is keyed by public key and contains `{ displayName, role, addedBy, addedAt }`. A unique genesis operation and every later parent-linked add or remove operation are signed by an identity authorized at the parent. Peers deterministically replay the complete authenticated operation set and validate signatures, parents, membership intervals, removal authority, and re-admission authority before applying a change. Each canonical `AddMember` operation ID opens a fresh interval.

- **RS.SYS.ID-R05 Member list updates are gossiped and recoverable.** Changes to the membership-operation set are gossiped to online workspace peers. Activation, neighbor arrival, and gossip lag request/rebroadcast the complete authenticated set so offline peers converge after reconnection. Signed departure requests and recipient-public-key records are supporting Iroh control data only and cannot change membership. `refines: RS.SYS.ID-R04`

### Invites

- **RS.SYS.ID-R06 Any member may generate a signed invite token.** A base58 invite encodes protocol version, workspace ID/token/display name, optional relay override, inviter public key, bootstrap peer address hint (the inviter's current Iroh endpoint), and inviter signature. `refines: RS-R04`

- **RS.SYS.ID-R07 Invite acceptance is a two-step join.** Accepting a token: (1) decode/validate it, establish recipient-key custody, and connect to the bootstrap peer, (2) send a signed join request containing the new member's public identity, display name, and conversation recipient-public-key record, (3) only the named inviter, while still authorized to add that interval, prepares the new member as `contributor` through a signed membership operation. The operation, resulting conversation epoch, and durable publication duties commit before gossip publication. If the bootstrap peer is offline or a required recipient key is missing, joining remains pending.

- **RS.SYS.ID-R08 Invite tokens are single-use by convention.** The protocol does not technically enforce single-use, but the reference UI presents invites as single-use. Teams that need multi-use invites (onboarding many people) share the token through a trusted channel with awareness that it can be used multiple times.

### Access control

- **RS.SYS.ID-R09 Unknown public keys are rejected.** Workspace-file operations and current control records from identities outside the canonical current membership are rejected. Conversation records additionally validate the named canonical historical epoch and the author's membership interval at that epoch; a current-member check alone cannot reject an otherwise eligible historical record. `refines: RS-R15`

- **RS.SYS.ID-R10 Role is enforced at the package level.** The runtime passes the member's role to packages on load. Packages hide or disable UI for operations above the member's role. Roles do not grant membership admission, removal, epoch, or conversation authority. The runtime does not otherwise enforce role semantics for package-defined operations; enforcement is the package's responsibility. `refines: RS-R03`

- **RS.SYS.ID-R11 Removal is creator-authored and atomically epoch-bound.** The immutable creator is derived from canonical genesis, never role. A non-creator may sign a departure request bound to its current interval; the genesis creator processes a valid request automatically and authors the removal. Only the creator may expel another member, and the creator cannot leave or be expelled in v1. The removal, resulting member projection, exact next conversation epoch, and durable Iroh/Commonware publication duties commit before publication. The requester never receives or opens the next epoch key. `refines: RS.SYS.ID-R04`

- **RS.SYS.ID-R14 Re-admission creates a fresh membership interval.** Any current member may re-admit after a valid requested departure. Only the genesis creator may re-admit after creator expulsion. An old request cannot remove a later interval, and roles do not alter this authority.

- **RS.SYS.ID-R12 Roles are a fixed set with extensible schema.** The initial roles are `viewer`, `contributor`, and `developer`. The member-list schema preserves unknown role strings but current clients treat them no more permissively than `viewer`, allowing later roles without unsafe interpretation. `refines: RS.SYS.ID-R04`

---

## Open Design Questions

- **RS.SYS.ID-DQ02** How does multi-device identity work? (Deferred to post-v1, but the member list schema should not preclude it.)
