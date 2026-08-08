# Design System

Owner: team

## Context

Resonance has grown from a single workspace into multiple packages — Backlog, Telemetry, Architecture, and more — each contributing UI surfaces with distinct capabilities. As we've built these out, we're seeing repeated patterns: low-level components (titles, collapsible lists, panels), mid-level compositions (agent chat interfaces, navigators, validation panels), and high-level surfaces (the Architecture workspace, the Telemetry dashboard, the Backlog interface).

This duplication is already visible across just a few packages, and it will accelerate as more packages are added. The goal of this decision is to give architectural lift to the front end — not to over-engineer it, but to make building new UI surfaces fast, deterministic, and easy to request from an agent.

## Current implementation

The first shared-browser slice landed in commit `aa779efb2b23ce6da8d8dbe60490ec506e5f0088`:

- `src/ui/agent-panel.js` owns the common agent-panel interaction model: transcript state, send/stop, retry, reset, credential handling, context usage, auxiliary content, visibility, and accessible form controls.
- `src/ui/collapsible-section.js` owns the accessible labelled toggle, `aria-expanded`/`aria-controls`, hidden item slot, and collapse indicator used by grouped workspace navigation.
- `src/ui/ui.css` provides the shared panel and collapsible-section styles. Shell serves it as `/assets/shell/ui.css`.
- Backlog and Architecture now author their browser entries as `*-source.js` files, bundle them with `bun run build:browser`, and adapt their domain state to the shared modules. Their routes, agent sessions, navigation data, and domain-specific message rendering remain package-owned.
- Shared module tests live in `src/ui/*.test.ts`; package tests continue to cover the Backlog and Architecture adapters.
- `docs/design-system.md` documents the current seam, modules, build convention, and verification approach.

This is an initial extraction, not the complete design system. The shared modules are JavaScript rather than typed TypeScript, there is no generic Panel/WorkspaceLayout library or package-workspace template yet, Telemetry has not been migrated, and no UI-scaffolding agent skill has been added. The Architecture `explain` skill added in the same commit is an architecture-agent capability, not a design-system scaffolder.

## What we want

A design system that makes Resonance's UI:

- **Extendable** — New packages should be able to contribute UI surfaces with minimal friction. Adding a new panel, view, or workspace should feel like wiring up a known pattern, not inventing from scratch.
- **Consistent** — Shared components, layout primitives, and interaction patterns so the application feels coherent even as it grows.
- **Agent-friendly** — The patterns should be well-documented and templated enough that an agent can scaffold a new UI surface from a short request, following deterministic conventions.

## Scope of this work

### 1. Audit and catalog existing UI patterns

Survey the existing packages (Backlog, Telemetry, Architecture, Shell, Workspace UI) and catalog:

- Low-level primitives: buttons, titles, collapsible sections, lists, badges, status indicators
- Mid-level compositions: navigators, panels, chat interfaces, validation result displays
- High-level surfaces: workspace layouts, agent panels, diagram renderers
- State patterns: how panels open/close, how selections propagate, how data flows between components

The initial audit covers the shared Backlog/Architecture agent panel and collapsible navigation patterns. Complete the broader Telemetry, Shell, and workspace audit before declaring this scope item complete, including the remaining near-duplicates and theme/token usage.

### 2. Define the component hierarchy and primitives

Establish a clear component taxonomy:

- **Foundations** — Typography, colors, spacing, icons, focus/accessibility tokens
- **Primitives** — Button, Collapsible, List, Badge, StatusDot, Panel, Title, Section, Tabs
- **Compositions** — Navigator, AgentChat, ValidationPanel, ViewSelector, DiagramPane
- **Layouts** — WorkspaceLayout, SplitPane, SidebarLayout
- **Templates** — Scaffold for a new package workspace (navigator + main pane + agent panel)

### 3. Build or extract shared components

The initial extraction moved the common agent panel and collapsible section from Backlog and Architecture into `src/ui/`, with package adapters retaining domain behavior. Continue extracting patterns as they prove themselves; do not rewrite package-specific surfaces merely to force reuse. Components should be:

- Well-typed (TypeScript)
- Accessible (ARIA)
- Themed via CSS variables or a design token system
- Documented with usage examples

The current modules satisfy the accessibility, CSS-token, and testable-seam goals, but remain JavaScript and need fuller usage examples and typed interfaces.

### 4. Create agent skills for UI scaffolding

Build Architecture agent skills (or extend existing ones) that can:

- Scaffold a new package workspace from a short description
- Add a new panel or view to an existing workspace
- Suggest the right component to use for a given UI pattern
- Generate the wiring code (routes, assets, package contract entries) for a new UI surface

### 5. Documentation and examples

- `docs/design-system.md` now documents the current shared modules, workspace seam, build convention, and verification.
- Add runnable examples or inline usage documentation for each component as the library grows.
- Document and implement the agent skills for UI scaffolding so developers know what to ask for.

### 6. Integration

- The team-owned shared browser modules are bundled into package browser entries, and Shell serves the shared stylesheet at `/assets/shell/ui.css`.
- Member-package distribution is intentionally not implicit; define a supported contract seam before making shared modules importable there.
- No manifest entry is needed because the design system is browser infrastructure, not a configured package.
- Backlog and Architecture have been migrated for common agent-panel and collapsible-section behavior; migrate Telemetry and other packages where the audit identifies a real shared pattern.

## Non-goals

- A full-blown design token system with a design tool integration (CSS variables are sufficient)
- Rewriting existing UI for the sake of purity — extract patterns as they prove themselves
- A separate Storybook deployment — inline documentation in the component library is sufficient
- CSS framework lock-in — the design system should be lightweight and replaceable

## Completion criteria

This decision is complete when a developer can:

1. 🟡 Read `docs/design-system.md` and understand the current shared-module seam, build convention, and verification approach. The current document does not yet describe a complete component hierarchy or every component's usage.
2. 🟡 Find the shared agent-panel and collapsible-section modules in `src/ui/` and see their accessible interfaces and colocated tests. Typed TypeScript interfaces, broader primitives, and complete usage examples are still missing.
3. 🔴 Request from the Architecture agent "add a new workspace for the Git package" and receive a scaffolded workspace with navigator, main pane, and agent panel using shared components. No UI-scaffolding skill or workspace template exists yet.
4. 🔴 Add a new panel to an existing workspace by importing a shared Panel component and wiring it into the layout. A generic shared Panel component has not been extracted; the current shared agent panel is a narrower composition.
5. 🟡 See that the existing packages use shared components for common patterns. Backlog and Architecture share agent-panel and collapsible-section behavior; Telemetry and the broader status/list/panel inventory remain to be audited or migrated.
6. 🟢 Run `bun test` and have all existing tests pass. The shared-module and package adapter tests pass as part of the full suite.
