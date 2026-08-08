# Resonance design system

The design system is browser infrastructure shared by package workspaces. It is not a configured package and does not contribute routes, navigation, or manifest entries.

## Current shared modules

Shared browser modules live in `src/ui/` and are bundled into team-package browser entries. Shell serves the shared stylesheet at `/assets/shell/ui.css`. Member-package distribution is intentionally not implicit: member packages must not import the viewed application root until the member contract provides a supported UI distribution seam.

### Agent panel

`createAgentPanel()` owns the common agent-panel interaction model:

- transcript rendering and scrolling
- ready, working, stopping, and error states
- send/stop, retry, reset, and credential actions
- context-usage and auxiliary-content slots
- visibility and accessible form controls

The package remains responsible for its agent session, routes, SSE adapter, domain context, and message composition. Use `renderTranscript` when a package needs a domain-specific transcript composition, as Architecture does for consecutive assistant responses.

### Collapsible section

`createCollapsibleSection()` creates an accessible section with a labelled toggle, `aria-expanded`, `aria-controls`, a hidden item slot, and a consistent collapse indicator. The caller owns persistence and item selection. It supports both grouped Backlog decisions and grouped Architecture views without knowing either domain.

## Package workspace seam

A workspace should be composed from domain-owned slots:

```text
navigator | main | agent (optional)
```

The shared modules must not know package route names, repository data, agent prompts, or domain identifiers. Package browser modules adapt their domain state to the shared interfaces.

## Build convention

Architecture and Backlog browser entries are authored as `*-source.js` files and bundled into their registered assets:

```sh
bun run build:browser
```

A package may omit the agent panel entirely. Future workspace starters should generate the layout and adapter seams, but domain routes, navigation models, message rendering, and agent capabilities remain bespoke.

## Verification

Shared module behavior is tested at the DOM seam in `src/ui/*.test.ts`. Backlog and Architecture retain their package-level browser tests, which verify that the two adapters preserve their domain-specific behavior while using the shared modules.
