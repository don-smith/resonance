# 0003 — Use Iroh as the P2P transport, not the Holepunch stack

Status: accepted (2026-08-21, Don Smith).

## Context

The workspace app requires P2P networking: peer discovery, hole-punching, relay fallback, and data replication. Two candidate ecosystems cover this space: the Holepunch stack (Hyperswarm, Hypercore, Autobase — used in Copland) and Iroh (a Rust P2P library from n0, the team that originally built the Dat/Hypercore protocol).

The app shell is Tauri (see decision 0002). This makes the runtime language Rust.

## Options

### Option A — Holepunch stack (Hyperswarm / Hypercore / Autobase)

The Holepunch stack is mature, battle-tested, and was used in Copland. It runs in Node.js or Bare (Holepunch's minimal JS runtime).

Problems with Tauri integration:
- Hyperswarm/Hypercore runs in Node or Bare. Tauri's backend is Rust. The two runtimes cannot share a process.
- Integration requires a sidecar: a separate Node/Bare process launched by Tauri, communicating over a local socket or stdin/stdout.
- A sidecar adds: a separate update track (the sidecar binary must be bundled and updated alongside the Tauri binary), a bespoke IPC protocol between Rust and Node, and additional process lifecycle management.
- Copland demonstrated a specific performance problem with Autobase: its causal-graph linearization becomes expensive on long histories. This is a structural issue with the Autobase model, not a configuration issue.

### Option B — Iroh — chosen

Iroh is a Rust P2P networking library from n0 (formerly the Dat/Hypercore team). It provides:
- `iroh::Endpoint` — QUIC-based connections with hole-punching and relay fallback.
- `iroh-blobs` — content-addressed blob replication.
- `iroh-gossip` — pub/sub over a topic key, suitable for workspace membership, channel discovery, and lightweight message gossip.

Iroh integrates as a Rust crate in the Tauri backend. No sidecar, no cross-runtime IPC.

The protocol design draws on Holepunch lessons:
- Append-only log semantics (from Hypercore) applied to conversation history, without Hypercore as the implementation.
- Multi-writer convergence (from Autobase) avoided in favor of Yjs CRDTs for documents (no linearization cost).
- Hyperswarm's DHT-based discovery model reflected in Iroh's gossip topic design.

Tradeoffs accepted:
- Iroh is newer and has a smaller ecosystem than the Holepunch stack. The n0 team is active and the library is production-used, but community resources are fewer.
- Iroh's gossip layer is not a full pubsub broker; it is suitable for small-to-medium teams. Large workspace (hundreds of members) behavior is not validated.

## Evidence

- Copland prototype confirmed Holepunch P2P works but identified Autobase linearization and sidecar complexity as problems.
- Iroh's hole-punching mechanism is based on the same STUN/TURN approach as Hyperswarm's DHT bootstrap. Functional equivalence for the team-scale use case is expected.
- Iroh is used in production by Iroh-based applications (e.g., iroh.computer tools) as of 2026.

## Consequences

- Copland's Holepunch code is not reused. Its protocol design decisions are reference material.
- The Iroh integration is a Tauri plugin (a Rust crate). It exposes Tauri commands for peer connection, document sync, and channel messaging.
- Yjs (not Autobase) handles multi-writer document convergence. Iroh provides the transport only.
- Teams that want to self-host a relay can run `iroh-relay` (available from the Iroh project).
