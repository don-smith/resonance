# Conversations: Requirements

Role: owns workspace-public channels, signed and encrypted message authoring, direct Commonware delivery and recovery, local archive persistence, authorization epochs, and device-local unread state.

---

## Assumptions

- **RS.SYS.CONV-A01 Conversations use Commonware over directly reachable networks.** The conversation mesh uses validated participant addresses on a mutually reachable LAN, VPN, Tailscale network, or equivalent private route. V1 provides no conversation relay, rendezvous, NAT traversal, or public-Internet reachability. Iroh remains responsible for workspace lifecycle, membership delivery, presence, and workspace-file transport. `refines: RS.SYS.TRNS-A01`

- **RS.SYS.CONV-A02 Workspace membership is conversation authority.** The runtime derives conversation authorization from the canonical historical membership lineage. Conversations keep no second member list, signing identity, role-based authority, or user-managed access authority. `refines: RS.SYS.ID-R04`

## Requirements

### Channels

- **RS.SYS.CONV-R01 Channels are workspace-scoped and public to current members.** Every v1 channel is discoverable, readable for eligible epochs, and postable by every current workspace member. Private channels and per-channel membership are deferred. `refines: RS-A05`

- **RS.SYS.CONV-R02 Any current member may create a named channel.** The runtime creates one empty `#general` channel when conversations become available in a workspace. The immutable workspace genesis creator owns that channel. `refines: RS.PROD-R11`

- **RS.SYS.CONV-R03 Channel creators control lifecycle.** Only a channel creator may extend that channel's create, rename, and terminal archive chain. Deterministic complete-record IDs resolve concurrent predecessor children and active-name claims. Archive preserves readable history and rejects new posts. `refines: RS.SYS.CONV-R01`

### Messages and access

- **RS.SYS.CONV-R04 Messages are canonical, attributed, authenticated, and encrypted.** Every message is a bounded Resonance-owned v1 record. XChaCha20-Poly1305 seals the exact Markdown body under its membership-epoch key with the canonical public header as associated data. The installation Ed25519 identity signs the sealed record, and BLAKE3 over the complete signed bytes is its immutable ID. Peers validate structure and authority before key lookup or authenticated open. `refines: RS-R07, RS.SYS.ID-R09`

- **RS.SYS.CONV-R05 Messages are append-only Markdown.** V1 supports Markdown text only and has no edit, delete, attachment, binary-content, reply, or thread operation. Reply ancestry requires a future message v2 that leaves v1 bytes and IDs unchanged. `refines: RS-R07`

- **RS.SYS.CONV-R06 Membership changes rotate forward-only access.** Every accepted canonical admission or removal creates a new conversation authorization epoch. A new member receives no earlier key or absent-interval record. A removed member receives no next key or future traffic. Eligible members may still accept records from canonical historical epochs after validating lineage, membership at that epoch, channel state, and local key possession. `refines: RS.SYS.ID-R11`

- **RS.SYS.CONV-R07 Canonical membership alone controls epoch recipients and mesh peers.** Address notices, recipient-public-key records, and departure requests are signed supporting data only. They cannot admit, remove, or retain a member. Roles grant no membership authority. `refines: RS.SYS.CONV-A02`

- **RS.SYS.CONV-R08 Conversation records use Commonware only.** Commonware carries epoch, channel, message, acknowledgement, and recovery records. Iroh carries membership operations plus bounded secret-free departure-request, address-notice, and recipient-public-key control. Iroh carries no channel, message, epoch, acknowledgement, recovery, wrapped-key, or epoch-secret bytes. `refines: RS.SYS.CONV-A01, RS.SYS.TRNS-R04`

### Departure and re-admission

- **RS.SYS.CONV-R09 Departure is creator-processed and interval-bound.** A non-creator signs a request bound to its current membership interval. The immutable genesis creator processes a valid request automatically, authors the removal, creates the next epoch without the requester, and durably commits the operation, epoch, and both publication duties before either transport publishes. A pending request does not change membership by itself.

- **RS.SYS.CONV-R10 The genesis creator cannot leave in v1.** Only the genesis creator may expel another current member. No identity may remove the creator. Creator exit, ownership transfer, workspace-admin groups, manual approval, and request cancellation require a later membership protocol.

- **RS.SYS.CONV-R11 Re-admission opens a fresh interval.** Any current member may re-admit after a valid requested departure. Only the genesis creator may re-admit after creator expulsion. An old interval-bound request cannot remove a later admission, and the returning installation receives no key or record from an interval in which it was absent.

### Persistence, recovery, and state

- **RS.SYS.CONV-R12 Messages persist locally before delivery.** The runtime commits author sequence, Lamport value, one exact encrypted archive record, and durable outbox in one workspace SQLite transaction before network delivery. Retry and recovery move the same bytes. Duplicate IDs are idempotent, while conflicting records for one author sequence are quarantined independent of arrival order.

- **RS.SYS.CONV-R13 Recovery is sparse and interval-authorized.** Recovery exchanges bounded per-author heads and missing ranges or sets rather than assuming a high-water mark proves dense possession. The responder derives eligibility from canonical membership intervals and never trusts a caller-supplied admission claim. `refines: RS.SYS.CONV-R06`

- **RS.SYS.CONV-R14 Unread state is local per device.** The runtime stores a per-channel read position and derives unread counts locally. It does not replicate read state or emit read receipts.

- **RS.SYS.CONV-R15 Synchronization state is finite and truthful.** `Offline` means another current member exists but no valid direct candidate is known. `WaitingToSync` covers durable duties, a missing current epoch, a pending departure, or no usable authorized peer. `Current` means no known local duty remains, not that every peer has received every record. A one-member workspace is locally current.

### Package boundary and notification

- **RS.SYS.CONV-R16 Conversations use a versioned semantic capability.** `conversations:v1` exposes bounded channel snapshots, paged attributed Markdown, creator-owned lifecycle operations, posting, local unread state, and synchronization state. It exposes no exact records, signatures, keys, recipient keys, addresses, membership snapshots, workspace tokens, SQL values, or transport handles. `refines: RS.SYS.PKG-R19, RS.SYS-R12`

- **RS.SYS.CONV-R17 Conversation changes emit secret-free invalidations.** A declared invalidation may identify the workspace, channel, and optional message ID but carries no message body or authority snapshot. Packages fetch current state through `conversations:v1`. `refines: RS.SYS.PKG-R04`
