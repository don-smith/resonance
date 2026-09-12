# Conversations

Resonance conversations are public to the current workspace membership. They synchronize encrypted, attributed Markdown directly between authorized installations on a reachable LAN, VPN, Tailscale network, or equivalent private route.

## Start in `#general`

A new conversation-ready workspace has an empty `#general` channel. Select **Conversations**, choose `#general`, write Markdown, and post. Every displayed message includes its author's workspace display name. Message bodies are encrypted before they enter the durable archive or network outbox.

## Manage public channels

Any current member can create a public channel. The member who created that channel can rename or archive it. Archive is terminal: the history remains visible, but the channel cannot be renamed or receive another message. Channel controls do not change workspace membership.

This release has no private channels, per-channel membership, direct messages, replies, threads, edits, deletes, attachments, or moderation approval flow.

## Unread state

Unread counts and **Mark read** positions belong only to this installation. They are stored in its private workspace database and are not sent as read receipts or synchronized to peers.

## Synchronization labels

- **Current** means Resonance knows of no local conversation work still waiting for delivery. A one-member workspace can be Current without a peer.
- **Waiting to sync** means durable work remains, the current encryption epoch is unavailable, a departure is waiting for the creator, or no authorized peer is currently usable.
- **Offline** means another current member exists but Resonance has no valid direct address candidate for that member.

Messages authored while a peer is unavailable stay in the durable outbox and reuse the same encrypted bytes after restart. Reconnection and sparse recovery do not require the author to post again.

## Direct-network limits

Conversation delivery uses authenticated direct Commonware connections. Iroh carries workspace membership plus bounded secret-free address, recipient-public-key, and departure controls; it does not carry conversation messages, epochs, acknowledgements, or recovery data.

This release does not provide a conversation relay, rendezvous service, NAT traversal, hole punching, STUN, TURN, or UPnP. Members need an already reachable route. Do not interpret **Waiting to sync** as a promise that an unreachable Internet peer can be contacted.

Admission and removal are forward-only after canonical membership converges. A newly admitted or re-admitted member cannot recover conversation keys or records from an interval in which they were absent. A removed member receives no next-epoch key or traffic. These rules do not promise retroactive erasure or post-compromise security.

## Private local data

Exact signed ciphertext records, retry duties, recovery state, and local unread positions stay in the private workspace database. Installation signing custody and the dedicated conversation-recipient key use separate native Keychain accounts; debug profiles use separate owner-only files. Packages receive only the bounded semantic `conversations:v1` views described in [Package authoring](package-authoring.md).

See [Local workspace data](local-data.md) for storage and custody details, or return to the [documentation index](index.md).
