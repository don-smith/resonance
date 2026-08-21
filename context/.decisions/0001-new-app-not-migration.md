# 0001 — Build a new app, not a migration of Resonance CLI or Copland

Status: accepted (2026-08-21, Don Smith).

## Context

Two prior prototypes exist: the Resonance CLI (a localhost HTTP server with a package-contract architecture for browsing a Git repository in a browser) and Copland (an Electron app using Hypercore/Hyperswarm/Autobase for P2P Markdown planning). The question was whether to evolve one of these into the collaborative workspace app, or start fresh.

Neither app has users. Resonance has been shown in demos; Copland is a learning prototype. Both have been validated at the idea level, not the production level.

## Options

### Option A — Evolve Resonance CLI
Add P2P sync, identity, and conversations to the existing HTTP-server model.

The HTTP server model is incompatible with a desktop app. It requires a browser; it binds to localhost; it has no process identity, no background sync, no system tray. Adapting it would require replacing its entire transport layer while preserving package behavior, with no clear incremental milestone.

### Option B — Evolve Copland
Replace Copland's Electron shell with Tauri, add Resonance's package ideas on top.

Copland's P2P layer (Holepunch stack) is Node/Bare-native and does not integrate cleanly with Tauri's Rust backend without a sidecar process. Its UI is minimal (unrendered Markdown, no rich editing). Its package model does not exist. Using it as the base means carrying forward a slow, immature P2P layer and building a package system around it.

### Option C — New app, extract ideas — chosen

Start a new Tauri project. Carry forward ideas, patterns, and lessons from both prototypes:
- Resonance: package-contract, event bus, team/member package distinction, AI-in-packages model.
- Copland: CRDT sync is the right approach for planning documents; Autobase's performance problems inform the choice to use Yjs instead.

Build the P2P layer with Iroh (Rust-native), the document layer with Yjs, the shell with Tauri.

## Evidence

- Copland prototype confirmed that CRDT sync over P2P is viable and the UX is compelling, but identified Autobase linearization as a performance bottleneck.
- Resonance CLI confirmed that the package-contract architecture works well for extensibility, and that the HTTP server model is wrong for a team app (it requires a running terminal, binds to one repo, has no identity).
- Iroh provides the same P2P primitives as the Holepunch stack in native Rust, removing the sidecar requirement.

## Consequences

- The Resonance CLI and Copland remain as independent prototypes. Their code is a reference, not a dependency.
- The new app uses the name "Resonance" (the CLI will be renamed or archived).
- The design documentation (`context/`) starts fresh. Prior `docs/` in the Resonance CLI repo are reference material only.
- No compatibility obligation to either prior app.
