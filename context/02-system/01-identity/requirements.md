# Identity — Requirements

Role: owns keypair generation and storage, workspace creation, invite token generation and acceptance, member list management, and access control enforcement.

---

## Assumptions

- **RS.SYS.ID-A01 One identity per installation.** A member has one Ed25519 keypair per machine. Multiple devices require multiple identities (device linking is a future concern).

- **RS.SYS.ID-A02 Workspace token is the shared secret.** Knowledge of the workspace token grants the ability to attempt to join the workspace. The member list is the second gate.

---

## Requirements

### Keypair management

- **RS.SYS.ID-R01 Keypair generated on first launch.** If no keypair exists in the OS keychain, one is generated on first launch before any workspace interaction. The private key never leaves the keychain.

- **RS.SYS.ID-R02 Public key is the stable identity.** The public key is used as the member's ID in all signed artifacts (messages, document updates, member list entries). Display names are advisory and may change; the public key does not.

### Workspace

- **RS.SYS.ID-R03 Workspace creation generates a random token.** The workspace token is 32 bytes of cryptographically random data. It is used as the Iroh gossip topic key for membership. `refines: RS-R02`

- **RS.SYS.ID-R04 The workspace member list is a signed set.** The member list is a map from public key to `{ displayName, role, addedBy, addedAt }`. Each update to the list is signed by an existing member. Peers validate signatures before applying member list changes.

- **RS.SYS.ID-R05 Member list updates are gossiped.** Changes to the member list are gossiped to all online workspace peers immediately. Offline peers receive the update on reconnection. `refines: RS.SYS.ID-R04`

### Invites

- **RS.SYS.ID-R06 Any member may generate an invite token.** An invite token encodes: workspace token, workspace display name, inviter's public key, a bootstrap peer address hint (the inviter's current Iroh endpoint). Encoded as base58. `refines: RS-R04`

- **RS.SYS.ID-R07 Invite acceptance is a two-step join.** Accepting a token: (1) decode and connect to the bootstrap peer, (2) send a signed join request containing the new member's public key and display name, (3) the receiving peer adds the new member to the member list and gossips the update. If the bootstrap peer is offline, the new member cannot join until an online peer is found.

- **RS.SYS.ID-R08 Invite tokens are single-use by convention.** The protocol does not technically enforce single-use, but the reference UI presents invites as single-use. Teams that need multi-use invites (onboarding many people) share the token through a trusted channel with awareness that it can be used multiple times.

### Access control

- **RS.SYS.ID-R09 Unknown public keys are rejected.** Messages and document updates signed by public keys not in the member list are silently dropped by receiving peers. `refines: RS-R15`

- **RS.SYS.ID-R10 Role is enforced at the package level.** The runtime passes the member's role to packages on load. Packages hide or disable UI for operations above the member's role. The runtime does not enforce role semantics for package-defined operations; enforcement is the package's responsibility. `refines: RS-R03`

- **RS.SYS.ID-R11 Member removal gossiped immediately.** When a member is removed, the updated member list (without their key) is gossiped to all online peers. Removed members who are online at the time of removal will see their contributions rejected by peers within one gossip round-trip.

- **RS.SYS.ID-R12 Roles are a fixed set with extensible schema.** The initial roles are `reviewer`, `contributor`, and `developer`. The member list schema must be designed to allow additional roles to be added in a future release without a breaking change. `refines: RS.SYS.ID-R04`

---

## Open Design Questions

- **RS.SYS.ID-DQ02** How does multi-device identity work? (Deferred to post-v1, but the member list schema should not preclude it.)
