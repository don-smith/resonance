# Conversations: Spec

## Status

Draft.

Accepted RFC 0011 and the P2P channel-chat design govern this planned implementation. This spec defines workspace-public conversation authority, canonical encrypted records, direct Commonware replication, local archives, forward-only interval access, and the package contract. It excludes direct messages, private channels, threads, attachments, edits, deletes, read receipts, creator exit, relay, and NAT traversal.

## 1. Runtime and authority boundary

The runtime starts one `ConversationRuntime` only for a ready active workspace. `WorkspaceSession` supplies canonical membership transitions; conversation code cannot create, authorize, or edit membership. `WorkspaceApplication` sequences the prepared membership transition through the conversation runtime and durable store before finalizing the live projection or scheduling transport publication.

The canonical membership head names an authorization epoch. The complete canonical lineage remains available for historical validation. Supporting departure requests, address notices, and recipient-public-key records are signed and workspace-bound but grant no membership authority. Role strings never grant admission or removal authority.

The immutable creator is the canonical genesis author. A non-creator may sign a self-removal request that binds the workspace, requester, current membership-interval ID, genesis creator, nonce, and advisory time. The genesis creator processes a valid request automatically and authors the removal. Only the genesis creator may expel another member, and the creator cannot leave or be expelled in v1.

Each canonical `AddMember` operation ID opens a fresh interval. Any current member may re-admit an identity after its requested departure; only the genesis creator may re-admit after creator expulsion. Requests from an old interval cannot affect a later one. A pending request does not change canonical membership, though the standard requester blocks new local conversation authoring until the request resolves.

## 2. Identity, recipient keys, and epochs

Commonware authentication reconstructs its Ed25519 signer from the same in-memory 32-byte installation secret used by Iroh and verifies that both public-key byte strings equal `PublicIdentity`. It creates and persists no second signing identity.

Each installation also owns one dedicated X25519 recipient keypair in separate native custody. A signed `ConversationRecipientKeyV1` carries the public key, workspace, installation identity, suite, and generation. It is accepted only as supporting data for a canonical current member and is removed from the active directory when that member leaves.

Before a locally authored membership operation changes the live projection, `WorkspaceSession` prepares and signs it against the current head without mutation. `ConversationRuntime` requires recipient-key records for exactly the resulting members, generates one random 32-byte epoch key, and creates an identity-sorted `SignedConversationEpochV1`. HPKE Base mode with X25519-HKDF-SHA256, HKDF-SHA256, and ChaCha20-Poly1305 wraps the same key for each resulting member with context binding the Resonance domain, versions, workspace, both membership heads, coordinator, recipient identity and public key, and recipient-key record ID.

One workspace-store transaction commits the membership operation, materialized members, exact epoch record, coordinator-own-envelope proof, request outcome when applicable, monotonic lookup peer-set version, Iroh membership-publication duty, and Commonware epoch-distribution duty. Projection and transport remain untouched until commit. The requester in a voluntary departure never generates, opens, or receives the next key. A missing key or signer, cryptographic failure, storage failure, or unavailable creator leaves the old head authoritative.

Admission and removal are forward-only. An admitted installation receives no old envelope or record from before its interval. A removed installation receives no next envelope or future recovery record. An existing member may accept an eligible message from a canonical historical epoch after checking lineage, author membership at that head, channel state, and possession of that epoch key. V1 does not prove that a malicious former member did not backdate a valid old-epoch record.

## 3. Canonical records and cryptography

Conversation records use a purpose-specific canonical module on pinned `commonware-codec`, not postcard or Hull. Every family has a Resonance marker, closed family tag, format and suite versions, fixed field order, canonical integer spelling, bounded bytes, bounded UTF-8 and vectors, and complete-extent consumption. Unknown kinds or versions, non-minimal integers, malformed UTF-8, trailing bytes, and bound violations are rejected. Accepted bytes decode and re-encode identically, and BLAKE3 over the complete signed bytes is the record ID.

`SignedConversationMessageV1` has a public header containing workspace and channel IDs, authorization epoch, observed channel head, author installation identity, author-local sequence, Lamport time, display creation time, fixed encryption and body-format IDs, and version. `MessageBodyV1` is bounded UTF-8 Markdown. XChaCha20-Poly1305 seals the exact body with a random 24-byte nonce under the epoch key and uses the domain-separated canonical header as associated data. The installation Ed25519 identity signs header, nonce, and ciphertext under `resonance.conversation-message.v1`.

The exact encrypted signed record is the archive and replication artifact. Retry and recovery never decrypt and re-encrypt it. Structural and authority validation precede key lookup and authenticated open. Typed failures distinguish malformed or unsupported input, limits, authorization, missing keys, recipient mismatch, authenticated-open failure, and local sealing failure.

Message v1 has no reply field. A future message v2 may add an optional parent ID while every v1 fixture, signature, and ID remains unchanged.

## 4. Channel projection and message archive

The genesis creator creates the signed empty `#general` record. Any current member may create another public channel. A channel lifecycle chain contains create, rename, and terminal archive records. Only its creator may extend it. Replay checks signature, workspace, membership at the named epoch, creator, predecessor, normalized name, and operation rules.

Concurrent children of one predecessor resolve by the lowest complete signed record ID. Concurrent active-name claims across channels use the lowest competing record ID. Losing, malformed, and unsupported exact bytes remain in bounded diagnostic custody without altering package-visible projection.

Local message authoring allocates author sequence and Lamport time, seals one exact record, and commits the counter, archive record, and durable outbox atomically. Stable display order is Lamport time, author bytes, author sequence, then message ID. Duplicate IDs are idempotent. Different records for one author sequence quarantine both and remove that sequence from accepted projection regardless of arrival order.

Read positions remain in the local workspace database and never replicate.

## 5. Transport, recovery, and operational state

Commonware authenticated lookup uses canonical current membership as its peer set and a separately validated address directory for direct candidates. Peer-set versions advance only for accepted canonical membership-head changes; address replacement does not advance membership. A removed peer is evicted immediately and cannot reconnect as an authorized member.

A production mesh runs on one named owned OS thread with a Commonware runtime, bounded command and event queues, explicit backpressure, startup and panic propagation, graceful stop, join, and restart behavior. SQLite, membership replay, and conversation cryptography remain outside that thread.

Iroh carries membership operations and bounded secret-free departure requests, address notices, and recipient-public-key records. It carries no channel, message, epoch, acknowledgement, recovery, wrapped-key, or epoch-secret bytes. Commonware alone carries every conversation record, acknowledgement, and recovery exchange.

Recovery exchanges bounded per-author heads and bounded sparse missing ranges or sets. The responder derives eligibility from canonical membership intervals rather than trusting the requester. Exact validated bytes commit before acknowledgement and recovery cursors advance.

`Offline` means another current member exists but no valid direct address candidate is known. `WaitingToSync` means durable work remains, the current epoch is missing, a departure awaits the creator, or no authorized peer is usable. `Current` means no known local duty remains; it does not assert global receipt. One-member workspaces are locally current.

## 6. Package boundary and events

A bundled conversation package declares `conversations:v1`. The bounded semantic capability lists channel snapshots, creates and changes creator-owned channels, pages attributed Markdown messages, posts, returns local unread counts, marks read, and reports finite synchronization state.

The runtime emits `conversations:changed` invalidations containing workspace ID, channel ID, and optional message ID only. The adapter validates responses, suppresses stale results, and owns listener cleanup. Neither capability nor event exposes exact records, signatures, keys, recipient keys, addresses, membership snapshots, workspace tokens, SQLite values, or transport handles.

V1 provides no removal approval UI, replies, threads, edits, deletes, attachments, direct messages, private channels, per-channel membership, read receipts, synchronized read state, relay fallback, rendezvous, hole punching, STUN, TURN, or UPnP for conversation traffic.
