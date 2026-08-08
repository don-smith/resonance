# Design System

Owner: team

## Context

Resonance has grown from a single workspace into multiple packages — Backlog, Telemetry, Architecture, and more — each contributing UI surfaces with distinct capabilities. As we've built these out, we're seeing repeated patterns: low-level components (titles, collapsible lists, panels), mid-level compositions (agent chat interfaces, navigators, validation panels), and high-level surfaces (the Architecture workspace, the Telemetry dashboard, the Backlog interface).

This duplication is already visible across just a few packages, and it will accelerate as more packages are added. The goal of this decision is to give architectural lift to the front end — not to over-engineer it, but to make building new UI surfaces fast, deterministic, and easy to request from an agent.

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

Identify every case of duplication or near-duplication.

### 2. Define the component hierarchy and primitives

Establish a clear component taxonomy:

- **Foundations** — Typography, colors, spacing, icons, focus/accessibility tokens
- **Primitives** — Button, Collapsible, List, Badge, StatusDot, Panel, Title, Section, Tabs
- **Compositions** — Navigator, AgentChat, ValidationPanel, ViewSelector, DiagramPane
- **Layouts** — WorkspaceLayout, SplitPane, SidebarLayout
- **Templates** — Scaffold for a new package workspace (navigator + main pane + agent panel)

### 3. Build or extract shared components

For each identified pattern, either extract from an existing package into a shared location or build a new shared component. Components should be:

- Well-typed (TypeScript)
- Accessible (ARIA)
- Themed via CSS variables or a design token system
- Documented with usage examples

### 4. Create agent skills for UI scaffolding

Build Architecture agent skills (or extend existing ones) that can:

- Scaffold a new package workspace from a short description
- Add a new panel or view to an existing workspace
- Suggest the right component to use for a given UI pattern
- Generate the wiring code (routes, assets, package contract entries) for a new UI surface

### 5. Documentation and examples

- Create `docs/design-system.md` explaining the component hierarchy, how to use each component, and how to contribute new ones
- Provide runnable examples or Storybook-style documentation for each component
- Document the agent skills for UI scaffolding so developers know what to ask for

### 6. Integration

- Ensure the shared component library is importable by all packages through the package contract system
- Wire the design system into the default `.resonance/config.json` example
- Update existing packages to use shared components where appropriate (migration, not rewrite)

## Non-goals

- A full-blown design token system with a design tool integration (CSS variables are sufficient)
- Rewriting existing UI for the sake of purity — extract patterns as they prove themselves
- A separate Storybook deployment — inline documentation in the component library is sufficient
- CSS framework lock-in — the design system should be lightweight and replaceable

## Completion criteria

This decision is complete when a developer can:

1. Read `docs/design-system.md` and understand the component hierarchy and how to use each component.
2. Find any shared component in the codebase and see its typed interface, accessibility attributes, and usage examples.
3. Request from the Architecture agent "add a new workspace for the Git package" and receive a scaffolded workspace with navigator, main pane, and agent panel using shared components.
4. Add a new panel to an existing workspace by importing a shared Panel component and wiring it into the layout.
5. See that the existing packages (Backlog, Telemetry, Architecture) use shared components for common patterns (collapsible lists, status indicators, panels) rather than each having their own.
6. Run `bun test` and have all existing tests pass.
