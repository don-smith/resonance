# 0011: Use Commonware for workspace conversations

Status: accepted (2026-09-04, Don Smith; removal protocol revised 2026-09-05).

## Context

Resonance needs workspace-public channels with durable, encrypted, attributed Markdown messages. Earlier conversation requirements assigned channel traffic and history recovery to Iroh Gossip and Iroh blobs. That conflicts with the selected Commonware direct-network direction and couples conversations to the workspace-file transport.

Workspace identity and membership already have one canonical authority: the causally signed membership log. A conversation node that keeps its own member list or signing identity would create a second authority. Conversation secrets, raw records, transport handles, and storage details must also remain behind the runtime and package-capability boundary.

V1 must work when authorized members have mutually reachable addresses on a LAN, VPN, Tailscale network, or equivalent private route. It does not promise relay fallback, rendezvous, NAT traversal, or public-Internet reachability.

## Options

### Option A. Keep Iroh as the conversation transport

This preserves the old Iroh Gossip channel-topic and blob-recovery model. It does not meet the selected Commonware direction and leaves conversation replication coupled to workspace lifecycle and file transport.

### Option B. Run an independent Commonware chat application

A separate node could own its own identity, members, persistence, and package bridge. It would duplicate workspace authority and require users to reconcile membership and access between two systems.

### Option C. Add a runtime-owned Commonware conversation service, chosen

The Rust runtime owns one workspace-scoped `ConversationRuntime`. It derives authorization from canonical membership transitions, uses the installation signing identity, retains recipient and epoch keys, persists exact encrypted records in the workspace database, owns Commonware lifecycle and recovery, and exposes only semantic `conversations:v1` operations to a bundled package.

Commonware authenticated lookup carries epoch, channel, encrypted-message, acknowledgement, and recovery records over validated directly reachable addresses. Iroh carries workspace membership operations and bounded, signed, secret-free conversation bootstrap control: departure requests, address notices, and recipient-public-key records. Iroh never carries channel or message records, acknowledgements, recovery records, epoch records, wrapped epoch keys, or epoch-secret bytes.

## Evidence

- `.myflow/workstreams/p2p-channel-chat/scope/20260904T125732Z_commonware-direct-network-v1.md` selects direct Commonware connectivity and excludes relay and NAT traversal.
- `.myflow/workstreams/p2p-channel-chat/research/20260904T125213Z_commonware-connectivity-capabilities.md` confirms that pinned Commonware lookup supports authenticated encrypted connections with application-owned peers and addresses.
- `.myflow/workstreams/p2p-channel-chat/research/20260904T214833Z_hull-commonware-record-and-crypto-patterns.md` supports a small canonical Commonware codec, XChaCha20-Poly1305 message bodies, X25519 HPKE epoch envelopes, validation-before-decryption, and golden compatibility fixtures. Hull and BLS time-lock encryption are unnecessary for this release.
- `.myflow/workstreams/p2p-channel-chat/design/20260904T215047Z_commonware-direct-chat-wire-and-crypto.md` settles the creator-processed departure protocol, immutable genesis creator, interval re-admission, and atomic membership-plus-epoch commit.
- `context/.decisions/0007-workspace-identity-membership-and-presence.md` and `crates/runtime/src/workspace_session/mod.rs` establish the signed membership projection as the sole workspace-member authority.
- `context/.decisions/0010-source-bundled-content-host.md` and `context/02-system/05-packages/spec.md` establish versioned semantic capabilities as the package boundary for privileged data domains.

## Consequences

- This decision supersedes Decision 0003 only for conversation traffic. Iroh still owns workspace lifecycle, membership delivery and recovery, presence, workspace files, hole punching, and relay fallback for its own duties. Iroh connectivity does not imply conversation connectivity.
- The conversation service uses Resonance-owned protocol, schema, module, crate, test, and UI names. Hull is research evidence only; Resonance has no Hull compatibility commitment.
- Durable conversation records use a purpose-specific canonical v1 format on exactly pinned `commonware-codec`. Closed record families have fixed tags, field order, canonical integers, bounded fields, complete-extent decoding, domain-separated signatures, and complete-record BLAKE3 IDs. Postcard remains the existing membership-envelope format, not the conversation record format.
- Message bodies are exact bounded Markdown bytes sealed with XChaCha20-Poly1305 under a membership-epoch key. The canonical public header is associated data. Retry and recovery preserve the exact signed encrypted record rather than re-encrypting it.
- One random key exists per canonical membership epoch. Each installation has a dedicated X25519 recipient key distinct from its installation Ed25519 signing key. HPKE Base mode with X25519-HKDF-SHA256, HKDF-SHA256, and ChaCha20-Poly1305 wraps the epoch key once for each resulting member.
- Commonware authenticates with an adapter reconstructed from the same in-memory 32-byte installation Ed25519 secret used by Iroh. The adapter proves matching public bytes and does not generate or persist another signing identity.
- Exact dependency pins freeze Commonware at `2026.7.1`, `hpke` at `0.14.0`, and the selected XChaCha implementation. `hpke 0.14.0` had no matching advisory in the checked RustSec snapshot and fits the toolchain and license, but its maintainers report no paid audit. The developer accepts that v1 assurance limit. This is not an MLS or post-compromise-security claim.
- A canonical membership transition defines an authorization epoch. Epoch recipient entries equal the resulting canonical member set exactly and are sorted by identity. Address notices, recipient-key records, and departure requests are supporting data only and cannot admit, remove, or retain a member.
- The immutable workspace creator is derived from canonical genesis, never from role. A non-creator may sign an interval-bound departure request. The genesis creator processes a valid request automatically, authors the removal, creates the next epoch without the requester, and commits the membership operation, epoch, own-envelope proof, and durable Iroh/Commonware publication duties before publication. The genesis creator cannot leave or be expelled in v1.
- Each admission opens a fresh membership interval identified by its canonical `AddMember` operation ID. Any current member may re-admit an identity after a requested departure. Only the genesis creator may re-admit an identity after creator expulsion. An old request cannot remove a later interval, and roles grant no membership authority.
- Admission and removal are forward-only. A joining member receives no earlier epoch key or absent-interval recovery record. A removed member receives no next-epoch key. Existing members may accept eligible records from canonical historical epochs after validating lineage, author membership at that epoch, channel state, and local key possession. V1 cannot prove that a malicious removed peer did not backdate an old-epoch record.
- The workspace genesis creator creates empty `#general`. Any current member may create a public channel, and that channel's creator alone may extend its create/rename/archive chain. Archive is terminal. Deterministic complete-record IDs resolve predecessor and active-name conflicts.
- Message v1 contains no reply or thread field. Replies require a future message v2 that leaves every v1 byte sequence, signature, and ID unchanged. Edits, deletes, attachments, direct messages, private channels, per-channel membership, read receipts, and synchronized read state are excluded.
- A local post commits its sequence, Lamport value, exact encrypted archive record, and durable outbox atomically before delivery. Duplicate IDs are idempotent; conflicting records for one author sequence are quarantined deterministically. Recovery tracks bounded sparse gaps rather than treating a high-water mark as dense possession.
- Synchronization state is finite and truthful. `Offline` means another current member exists but no valid direct address is known. `WaitingToSync` covers durable duties, missing current epochs, pending departures, and unusable authorized peers. `Current` means no known local work remains; it does not claim global delivery. A one-member workspace is locally current.
- The runtime adds durable conversation storage, `conversations:v1` contract fixtures, a bundled conversation package, secret-free invalidation events, and two-profile direct-network evidence. Exact records, signatures, keys, recipient keys, addresses, membership snapshots, workspace tokens, SQL values, and transport handles never cross the package boundary.
- HPKE wrapping grows linearly with current membership. V1 freezes member and record-size bounds and does not claim large-group efficiency, relay fallback, NAT traversal, post-compromise security, retroactive erasure, or instant revocation on an offline installation.
