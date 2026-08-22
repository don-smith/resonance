# Transport — Spec

## Status

Active.

This spec defines the active-workspace Iroh endpoint/topic lifecycle and its relationship to membership and shell presence. It excludes blob replication and document/conversation subtopics.

## 1. Endpoint lifecycle and relay

The runtime creates one Iroh 1.0.3 endpoint for the application lifetime with the installation identity key supplied to its builder. The endpoint and Router are owned by the runtime transport module. The active workspace's absent relay override uses Iroh `RelayMode::Default`; a validated configured URL uses a custom relay map. Changing the active workspace or its relay configuration restarts the transport. Shutdown stops Gossip, then awaits Router shutdown so protocol handlers and the endpoint close cleanly.

## 2. Root topic and bootstrap

The active workspace creates an Iroh Gossip 0.101.0 handler, registers it with the Iroh Router, and joins one root topic keyed by a domain-separated digest of its 32-byte workspace token. An invite's `NodeAddr` is registered before its endpoint ID is used as the initial Gossip bootstrap. Iroh types remain internal to the transport module; callers operate on workspace-scoped messages and observations.

## 3. Authenticated delivery and recovery

Membership operations, membership sync/request, and presence heartbeats are versioned protocol envelopes. They are signed by the sender's installation identity with a domain-separated canonical payload. The identity session validates sender membership and envelope signature before processing normal traffic. Join requests are the narrow exception: only their named, current-member inviter may process them.

Gossip is delivery, not durable history or membership authority. `Lagged`, workspace activation, and neighbor arrival initiate membership-sync recovery. The transport accepts incoming gossip via its Router and reports neighbor/path observations to the workspace session.

## 4. Presence

Every active canonical member emits a signed heartbeat on the root topic at a fixed cadence. A recent valid heartbeat makes that member `online`; TTL expiry makes it `offline`. A gossip neighbor/path observation refines an online member to `direct` or `relayed` when Iroh reports a current connection path. Gossip's bounded overlay neighbors must not be presented as a complete online-member list.

The runtime exposes a secret-free active-workspace read model and emits `peer:joined`, `peer:left`, and `peer:connection` transitions. Presence for unknown, removed, or invalidly signed members is suppressed.
