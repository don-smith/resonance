# Transport — Requirements

Role: owns the Iroh P2P layer: endpoint management, peer connection, hole-punching, relay fallback, gossip topics, and blob replication. It provides membership delivery, identity presence, authenticated workspace-file recovery, and bounded secret-free conversation bootstrap control; the conversation domain owns its direct Commonware replication.

---

## Assumptions

- **RS.SYS.TRNS-A01 Iroh is the workspace transport implementation.** See decisions 0003 and 0011. Iroh owns workspace lifecycle, membership delivery and recovery, presence, workspace-file transport, and bounded secret-free conversation bootstrap control. Conversation records use a separately managed direct Commonware mesh. Neither transport exposes raw types to packages; packages interact through semantic capabilities, events, and Tauri commands.

- **RS.SYS.TRNS-A02 The workspace token is the primary discovery key.** Peers find each other by joining the Iroh gossip topic derived from the workspace token.

---

## Requirements

### Connection management

- **RS.SYS.TRNS-R01 One Iroh endpoint per app instance.** The endpoint is created on startup and lives for the app lifetime. It uses the active workspace's relay configuration: Iroh's public production relay mode by default, or a validated workspace override. Changing the active relay configuration restarts transport. `refines: RS-R02`

- **RS.SYS.TRNS-R02 Peer connections are established automatically.** When a validated member is discovered on the active workspace topic, the runtime joins the Iroh gossip overlay and observes the available direct or relay path. `refines: RS-T01`

- **RS.SYS.TRNS-R03 Connection status is available to packages via events.** The runtime emits `peer:joined`, `peer:left`, and `peer:connection` events with a public member identifier and secret-free presence/path state. Packages subscribe to these events for presence UI.

### Gossip

- **RS.SYS.TRNS-R04 Each workspace uses one root gossip topic.** The root topic ID is a domain-separated digest of the 32-byte workspace token. It carries membership operations and recovery, signed presence, workspace-file history notices or recovery requests, and bounded signed conversation departure requests, direct-address notices, and recipient-public-key records. File history and bytes use authenticated streams. Iroh never carries conversation channel, message, epoch, acknowledgement, recovery, wrapped-key, or epoch-secret bytes.

- **RS.SYS.TRNS-R05 Gossip messages are signed.** Every normal gossip message includes the sender's public key and a domain-separated signature. Receivers verify the signature and check the sender against the canonical member list before processing; the named-inviter join request is the narrow onboarding exception. `refines: RS.SYS.ID-R09`

### Blob replication

- **RS.SYS.TRNS-R06 Blobs are content-addressed.** Workspace-file revision bytes and other static content are stored and transferred as content-addressed blobs. Receiving peers verify the declared hash before accepting.

- **RS.SYS.TRNS-R07 Blob transfer is on-demand.** Peers request workspace-file blobs when needed. Conversation records and recovery responses are not Iroh blobs. The runtime does not proactively push workspace-file blobs to new peers; it responds to authorized requests.

### Relay

- **RS.SYS.TRNS-R08 Relay URL is configurable.** The optional relay URL is persisted in workspace configuration and carried by signed invites, not compiled into the app. Its absence selects Iroh's public production relay mode. Teams may self-host `iroh-relay` and configure the URL in their fork. `refines: RS-T01`

- **RS.SYS.TRNS-R09 Relay carries no content authority.** The Iroh relay forwards encrypted QUIC traffic for Iroh duties. It cannot read workspace-file content and is not a fallback for v1 Commonware conversations. Relay operators can observe connection metadata (who connected to whom, when) but not encrypted Iroh content.
