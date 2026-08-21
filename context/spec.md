# Resonance — Context Spec

This document specifies the structure, ID scheme, and conventions of the `context/` tree. It is the meta-document for the VRS system itself.

## Status

Draft.

---

## Purpose

The `context/` tree is the single authoritative intent layer for Resonance. It answers "what should the system do and why." The implementation answers "what the system does." The gap between them is tracked in `.delta/`.

User-facing documentation derives from this tree; it never defines contracts that conflict with it.

---

## Tree structure

```
context/
├── vision.md              # Goals, anti-goals, success criteria
├── requirements.md        # RS-R*, RS-A*, RS-T* root requirements
├── spec.md                # This file — context structure and conventions
├── ontology.md            # Canonical terminology
├── open-questions.md      # Design uncertainties not yet resolved
├── .decisions/            # NNNN-slug.md — accepted architectural decisions
├── .delta/                # DELTA-NNN-slug.md — known debt and divergence
├── .reference/            # External platform guarantees this system depends on
├── 01-product/            # Positioning, personas, fit/non-fit
├── 02-system/             # Technical subsystems
│   ├── 01-identity/       # Keypair identity, workspace, invite
│   ├── 02-transport/      # Iroh P2P layer
│   ├── 03-documents/      # CRDT document sync (Yjs + TipTap)
│   ├── 04-conversations/  # Chat channels, append-only log
│   ├── 05-packages/       # Package model, event bus, contract
│   └── 06-repos/          # Git watching, repo registration, repo packages
├── 03-delivery/           # Auto-update, CI, signing, release
└── 04-contributing/       # Fork guide, RFC process, package authoring
```

Numeric prefixes encode dependency direction: higher-numbered nodes may depend on lower-numbered nodes; the reverse is not permitted.

---

## ID scheme

All IDs use the prefix `RS` (Resonance). IDs are globally unique across the tree.

| Namespace | Node | ID format |
|-----------|------|-----------|
| `RS-*` | Root | Requirements: `RS-R01`…, Assumptions: `RS-A01`…, Tradeoffs: `RS-T01`… |
| `RS.PROD-*` | `01-product/` | `RS.PROD-R01`, `RS.PROD-DQ01` |
| `RS.SYS-*` | `02-system/` | `RS.SYS-R01` |
| `RS.SYS.ID-*` | `02-system/01-identity/` | `RS.SYS.ID-R01`, `RS.SYS.ID-DQ01` |
| `RS.SYS.TRNS-*` | `02-system/02-transport/` | `RS.SYS.TRNS-R01` |
| `RS.SYS.DOC-*` | `02-system/03-documents/` | `RS.SYS.DOC-R01` |
| `RS.SYS.CONV-*` | `02-system/04-conversations/` | `RS.SYS.CONV-R01` |
| `RS.SYS.PKG-*` | `02-system/05-packages/` | `RS.SYS.PKG-R01` |
| `RS.SYS.REPO-*` | `02-system/06-repos/` | `RS.SYS.REPO-R01` |
| `RS.DEL-*` | `03-delivery/` | `RS.DEL-R01` |
| `RS.CONTRIB-*` | `04-contributing/` | `RS.CONTRIB-R01` |

IDs within a namespace are sequentially numbered. If an ID is removed or renumbered, the change is made in a single commit with no other changes.

---

## Document conventions

### requirements.md

Every node that owns requirements has a `requirements.md` with this structure:

```
# <Node> — Requirements

Role: [one sentence describing what this node owns]

## Assumptions        (RS.SYS.ID-A01…)
## Acceptable Tradeoffs  (RS.SYS.ID-T01…)
## Requirements       (RS.SYS.ID-R01…)
  grouped by theme, with a bold ID per bullet
## Open Design Questions  (RS.SYS.ID-DQ01…)
```

A requirement that constrains a parent requirement ends with `` `refines: <parent-id>` ``.

### spec.md

Every node that has a non-trivial implementation has a `spec.md` with this structure:

```
# <Node> — Spec

Status: Draft | Active | Stable.

[Scope paragraph: what this spec defines and what it excludes]

[Numbered sections, one per requirement group]
```

A `spec.md` in Draft status may change without notice. Active means it is implemented and tested. Stable means it has been in production use without breaking changes for at least one release cycle.

### .decisions/NNNN-slug.md

```
# NNNN — <Decision title>

Status: accepted (<date>, <who decided>).

## Context
[Why this decision needed to be made]

## Options

### Option A
[Description and tradeoffs]

### Option B — chosen
[Description and why chosen]

## Evidence
[What confirms the choice: prototype, benchmark, prior art, user feedback]

## Consequences
[What this decision forecloses or obligates]
```

### .delta/DELTA-NNN-slug.md

```
# DELTA-NNN — <Title>

Status: open | closed (<date>, <how resolved>).

## What the intent says
[The requirement or spec claim]

## What the implementation does
[The actual current behavior]

## Why the gap exists
[Cause: deadline, complexity, unresolved question, deliberate deferral]

## Resolution path
[What would close this delta]
```

---

## Enforcement

A Vitest suite (to be added in Phase 1) checks:

- All IDs referenced in `refines:` markers resolve to a real requirement.
- No duplicate IDs exist across the tree.
- Every `spec.md` has a `## Status` line.
- Every `.decisions/` file has a `Status:` line.
- Every `.delta/` file has a `Status:` line with `open` or `closed`.

The suite runs in CI and blocks merge on failure. It does not enforce content quality — only structural integrity.
