# Resonance roadmap

## Status

Active.

This file records the expected order of major work and the reason for that order. It does not replace requirements, specifications, accepted decisions, deltas, or workstream artifacts. The `context/` tree remains the source of product and architecture intent. MyFlow artifacts record execution details and verification for each workstream.

## Current baseline

The following foundations are complete:

- Tauri desktop runtime, local persistence, and verified update foundation.
- Workspace identity, signed membership, invite admission, and peer presence.
- Signed workspace-file authority, immutable blobs, private local-root projection, conflict handling, and P2P recovery.
- A generated bundled-package catalog, package lifecycle, semantic capability model, workspace-files package, and reference package.

The codebase now has one complete capability-backed content domain. The next work should make the architecture explicit before a second collaborative domain copies or extends the current patterns.

## Expected sequence

| Order | Work | Status | Outcome |
|---|---|---|---|
| 1 | Architecture assessment | Next | Describe and assess the current architecture, intended architecture, seams, dependencies, data flows, responsibilities, extensibility points, and trade-offs. |
| 2 | Architecture alignment | Planned | Triage the assessment, accept or reject each recommendation, and implement only the preparatory changes needed to make the architecture consistent and ready for another collaborative domain. |
| 3 | Conversations | Planned | Add the conversation domain as the second capability-backed content package, using signed append-only messages, local persistence, Iroh replication, and bounded history recovery. |
| 4 | Repository registration and packages | Later | Register local Git clones and load repository-backed views without adding Resonance replication for repository content. |

These are separate workstreams. In particular, the assessment does not silently become a refactor, and assessment findings are not accepted until the developer triages them.

## Architecture assessment expectations

The assessment must inspect the implementation broadly enough to explain the whole application rather than one isolated package or crate. It should produce durable Markdown source and a clear, self-contained HTML review packet. C4 views may describe system context, containers, and important modules, but C4 alone is insufficient.

The assessment should include:

- a map of runtime, desktop, package, persistence, and transport responsibilities;
- dependency direction and important module relationships;
- an inventory of interfaces, seams, and their adapters;
- startup, request, event, local mutation, persistence, and remote replication flows;
- the intended extensibility process for a new content package or data domain;
- the package trust model and the distinction between bundled, future member, and repository packages;
- current architecture versus intended architecture, with code evidence for each difference;
- trade-offs, pressure points, and likely conversation-domain preparation;
- recommendations grouped for explicit acceptance, rejection, or deferral.

The assessment should use diagrams where relationships matter. Dependency diagrams, sequence diagrams, and data-flow diagrams should supplement C4 views rather than force every concern into C4 notation.

Create a dedicated architecture-assessment skill before starting the review. It should reuse the exhaustive inspection and triage discipline of architecture-review, the seam vocabulary of codebase-design, and the artifact system of html-design. It must add architecture description, C4 views, data flows, extensibility, current-versus-intended comparison, and tracked context maintenance rather than changing the more general architecture-review skill into a Resonance-specific workflow.

## Architecture alignment expectations

The alignment workstream starts from triaged findings. Its purpose is not to redesign the application by default. It may clarify the existing design, make dependency direction enforceable, reduce accidental coupling, or change an architectural choice when the assessment establishes a concrete reason.

Before conversations begin, the codebase should make these points clear:

- which module owns workspace membership and lifecycle;
- how independently owned data domains attach to workspace lifecycle and transport;
- how domain protocols, persistence, background work, and transitions remain local to their domain;
- how a semantic capability connects a package to privileged runtime behavior;
- which events are notifications and which interfaces return authoritative state;
- which Rust and TypeScript modules are intended interfaces versus implementation details.

Any accepted architectural change must update the relevant requirements, specification, ontology, decision, or delta in the same workstream.

## Conversation direction

Conversations are the next major capability after architecture alignment. They should prove that the package and runtime architecture supports a second data domain with different authority and replication semantics from workspace files.

The first useful conversation release should center on a conversation package, a versioned semantic capability, a default `#general` channel, signed Markdown messages, local SQLite persistence, live gossip, restart and reconnect behavior, history catch-up, and local unread state. Detailed scope remains for the conversations workstream.

## Repository direction

Repository content remains Git-only. Resonance will not replicate repository files, snapshots, or repository-derived read-only views to a device that does not have a local clone. Repository packages may read a registered local clone through bounded runtime operations and react to local Git or filesystem changes. Members without a clone use workspace files and conversations, not a peer-provided repository view.

## Maintenance rule

Update this roadmap when a major workstream starts, closes, changes order, or changes the stated direction. Update subsystem requirements and specs when behavior or architecture intent changes. Record hard-to-reverse trade-off decisions in `context/.decisions/`, unresolved design questions in `context/open-questions.md`, and known implementation divergence in `context/.delta/`.
