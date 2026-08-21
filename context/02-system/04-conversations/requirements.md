# Conversations — Requirements

Role: owns conversation channels, message authoring, signature verification, append-only log replication, local persistence, and history catch-up for new members.

---

## Requirements

### Channels

- **RS.SYS.CONV-R01 Channels are workspace-scoped.** A channel belongs to a workspace, not a repository. Any workspace member may read any channel they have joined. `refines: RS-A05`

- **RS.SYS.CONV-R02 Any member may create a channel.** Channel creation is not restricted to a role. The creating member is the initial member; other workspace members discover the channel via workspace gossip and may join.

- **RS.SYS.CONV-R03 Channel membership is implicit for workspace members.** When a new channel is announced on the workspace gossip topic, all workspace members are notified. Joining is automatic (for public channels) or requires an accept step (for private channels, deferred). In v1, all channels are public within the workspace.

### Messages

- **RS.SYS.CONV-R04 Messages are signed by the sender.** Every message carries the sender's public key and a signature over the message content and timestamp. Peers verify the signature and member list membership before accepting. `refines: RS-R07`

- **RS.SYS.CONV-R05 Messages are append-only.** The core protocol has no delete or edit operation. A soft-delete flag (a second message that references the original by hash) is acceptable for UX purposes but does not alter the replicated log. `refines: RS-R07, RS-T02`

- **RS.SYS.CONV-R06 Message content is Markdown.** Messages are authored in Markdown and rendered on display. No binary attachments in v1.

- **RS.SYS.CONV-R07 Messages are gossiped on the channel topic.** Each channel has an Iroh gossip sub-topic derived from the channel ID. Messages are gossiped as signed payloads to all peers on the topic. `refines: RS.SYS.TRNS-R04, RS.SYS.TRNS-R05`

### Persistence and replication

- **RS.SYS.CONV-R08 Messages are persisted locally in SQLite.** Received and sent messages are appended to a local SQLite table keyed by channel ID and message hash. The message hash is the content hash of the signed payload.

- **RS.SYS.CONV-R09 History is replicated for new members.** When a peer joins a channel, it requests the channel's compacted log snapshot from an available online peer. The snapshot is an Iroh blob of all messages up to a compaction point; live messages are gossiped after. `refines: OQ-07`

- **RS.SYS.CONV-R10 Unread state is local.** The runtime tracks the last-read message hash per channel per member in local SQLite. Unread counts are computed locally. Peers do not sync read state.

### Notification

- **RS.SYS.CONV-R11 New messages emit a bus event.** On receiving a new message in any joined channel, the runtime emits a `message:received` event on the Tauri event bus with the channel ID, sender public key, and message hash (not content, to avoid routing sensitive data through the bus). Packages use this event to update unread indicators and trigger notifications.

- **RS.SYS.CONV-R12 Threading uses reply-to references.** Channels support hierarchical conversation threading. A message may carry a `reply_to` field referencing the hash of a parent message, forming a tree. Threading UX is deferred from v1; the protocol field is reserved for future use.


