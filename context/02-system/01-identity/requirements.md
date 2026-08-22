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

- **RS.SYS.ID-R02 Public key is the stable identity.** The public key is used as the member's ID in all signed artifacts (messages, document updates, member list entries). Display names are advisory and may change; the public key does not.

### Workspace

- **RS.SYS.ID-R03 Workspace creation generates a random token.** The workspace token is 32 bytes of cryptographically random data. A domain-separated digest of it supplies both the Iroh gossip topic ID and opaque local workspace ID. `refines: RS-R02`

- **RS.SYS.ID-R04 The workspace member list is a causally signed set.** The derived member map is keyed by public key and contains `{ displayName, role, addedBy, addedAt }`. A unique genesis operation and every later parent-linked operation are signed by a member authorized at the parent. Peers deterministically replay the complete authenticated operation set and validate signatures, parents, and authorization before applying a change.

- **RS.SYS.ID-R05 Member list updates are gossiped and recoverable.** Changes to the membership-operation set are gossiped to online workspace peers. Activation, neighbor arrival, and gossip lag request/rebroadcast the complete authenticated set so offline peers converge after reconnection. `refines: RS.SYS.ID-R04`

### Invites

- **RS.SYS.ID-R06 Any member may generate a signed invite token.** A base58 invite encodes protocol version, workspace ID/token/display name, optional relay override, inviter public key, bootstrap peer address hint (the inviter's current Iroh endpoint), and inviter signature. `refines: RS-R04`

- **RS.SYS.ID-R07 Invite acceptance is a two-step join.** Accepting a token: (1) decode/validate it and connect to the bootstrap peer, (2) send a signed join request containing the new member's public key and display name, (3) only the named inviter, while still a member, adds the new member as `contributor` through a signed membership operation and gossips it. If the bootstrap peer is offline, the new member cannot join until an online peer is found.

- **RS.SYS.ID-R08 Invite tokens are single-use by convention.** The protocol does not technically enforce single-use, but the reference UI presents invites as single-use. Teams that need multi-use invites (onboarding many people) share the token through a trusted channel with awareness that it can be used multiple times.

### Access control

- **RS.SYS.ID-R09 Unknown public keys are rejected.** Messages and document updates signed by public keys not in the member list are silently dropped by receiving peers. `refines: RS-R15`

- **RS.SYS.ID-R10 Role is enforced at the package level.** The runtime passes the member's role to packages on load. Packages hide or disable UI for operations above the member's role. The runtime does not enforce role semantics for package-defined operations; enforcement is the package's responsibility. `refines: RS-R03`

- **RS.SYS.ID-R11 Member removal gossiped immediately.** When a removal operation becomes canonical, it is gossiped to all online peers. Removed members who are online at the time of removal will see later contributions rejected by peers within one gossip round-trip.

- **RS.SYS.ID-R12 Roles are a fixed set with extensible schema.** The initial roles are `viewer`, `contributor`, and `developer`. The member-list schema preserves unknown role strings but current clients treat them no more permissively than `viewer`, allowing later roles without unsafe interpretation. `refines: RS.SYS.ID-R04`

---

## Open Design Questions

- **RS.SYS.ID-DQ02** How does multi-device identity work? (Deferred to post-v1, but the member list schema should not preclude it.)
