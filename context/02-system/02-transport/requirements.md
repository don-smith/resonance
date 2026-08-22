# Transport — Requirements

Role: owns the Iroh P2P layer: endpoint management, peer connection, hole-punching, relay fallback, gossip topics, and blob replication. Provides the transport substrate for identity gossip, document sync, and conversation replication.

---

## Assumptions

- **RS.SYS.TRNS-A01 Iroh is the transport implementation.** See decision 0003. The transport layer does not expose Iroh-specific types to packages; packages interact with transport through the event bus and Tauri commands.

- **RS.SYS.TRNS-A02 The workspace token is the primary discovery key.** Peers find each other by joining the Iroh gossip topic derived from the workspace token.

---

## Requirements

### Connection management

- **RS.SYS.TRNS-R01 One Iroh endpoint per app instance.** The endpoint is created on startup and lives for the app lifetime. It uses the active workspace's relay configuration: Iroh's public production relay mode by default, or a validated workspace override. Changing the active relay configuration restarts transport. `refines: RS-R02`

- **RS.SYS.TRNS-R02 Peer connections are established automatically.** When a validated member is discovered on the active workspace topic, the runtime joins the Iroh gossip overlay and observes the available direct or relay path. `refines: RS-T01`

- **RS.SYS.TRNS-R03 Connection status is available to packages via events.** The runtime emits `peer:joined`, `peer:left`, and `peer:connection` events with a public member identifier and secret-free presence/path state. Packages subscribe to these events for presence UI.

### Gossip

- **RS.SYS.TRNS-R04 Each workspace uses one root gossip topic.** The root topic ID is a domain-separated digest of the 32-byte workspace token. It carries membership operations/recovery, signed presence, channel discovery, and document update notifications. High-frequency data (document updates, messages) uses per-document or per-channel sub-topics.

- **RS.SYS.TRNS-R05 Gossip messages are signed.** Every normal gossip message includes the sender's public key and a domain-separated signature. Receivers verify the signature and check the sender against the canonical member list before processing; the named-inviter join request is the narrow onboarding exception. `refines: RS.SYS.ID-R09`

### Blob replication

- **RS.SYS.TRNS-R06 Blobs are content-addressed.** Static content (Yjs snapshots, conversation history compactions, repo content for non-developer peers) is stored and transferred as content-addressed Iroh blobs. Receiving peers verify the hash before accepting.

- **RS.SYS.TRNS-R07 Blob transfer is on-demand.** Peers request blobs when they need them (e.g., on channel join, on workspace join). The runtime does not proactively push blobs to new peers; it responds to requests.

### Relay

- **RS.SYS.TRNS-R08 Relay URL is configurable.** The optional relay URL is persisted in workspace configuration and carried by signed invites, not compiled into the app. Its absence selects Iroh's public production relay mode. Teams may self-host `iroh-relay` and configure the URL in their fork. `refines: RS-T01`

- **RS.SYS.TRNS-R09 Relay carries no content authority.** The relay forwards encrypted QUIC traffic. It cannot read message content or document data. Relay operators can observe connection metadata (who connected to whom, when) but not content.
