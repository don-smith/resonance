# Prezzos

Owner: team

## Context

Resonance needs a workspace for creating and delivering presentations. This is a natural fit for the package workspace pattern — navigator, main pane, and agent panel — that the Design System is standardizing.

## Approach

The presentation engine should be agent-friendly: deterministic, text-based, and easy for an agent to generate and iterate on. Candidates to evaluate:

- **MARP** — Markdown-based presentation framework. Strong candidate: Markdown is the most natural format for an agent to produce, and MARP's slide-deck-from-markdown model maps directly to agent-generated content.
- **tldraw** — A whiteboard/canvas approach. Interesting for visual, freeform presentations but less deterministic for agent generation. Could be a complementary view mode rather than the primary engine.
- **Reveal.js** — HTML-based presentations. More flexible than MARP but less structured for agent output.
- **Asciidoc + reveal.js** — Another text-based option worth evaluating.

The initial investigation should focus on MARP as the primary candidate, with a brief evaluation of whether tldraw could serve as an alternative or complementary presentation mode.

## Scope

1. **Evaluate presentation engines** — Compare MARP, tldraw, Reveal.js, and any other relevant options for agent-friendliness, output quality, and integration effort.
2. **Scaffold the Prezzos package** — Follow the Design System's package workspace pattern (navigator + main pane + agent panel).
3. **Implement presentation creation flow** — Agent generates slide content, user reviews and iterates, then delivers.
4. **Delivery mode** — Present slides full-screen or embedded within the workspace.

## Non-goals

- A full presentation editor with drag-and-drop slide editing
- Import/export from PowerPoint or Google Slides
- Animation or transition effects beyond what the engine provides natively

## Completion criteria

1. A Prezzos package workspace exists following the Design System conventions
2. An agent can generate a multi-slide presentation from a short description
3. The user can view and navigate slides within the workspace
4. A full-screen presentation mode works
