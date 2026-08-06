---
name: package-authoring
description: Use when creating, scaffolding, configuring, or changing a Resonance package, including when choosing between a starter and bespoke package work.
---

# Author Resonance Packages

## Core Rule

A Resonance package is application-root source plus an explicit viewed-repository manifest opt-in. Never create package source below the viewed repository, auto-load a package, or edit `.resonance/config.json` on the developer’s behalf.

## Capture the Brief First

Before selecting a starter or invoking the CLI, capture:
- **Purpose** — the user problem and the package’s bounded responsibility.
- **Data ownership** — repository files/data it reads or writes, and the read-only boundary.
- **Route, configuration, and UI** — named API routes, package inputs, navigation, and what renders in the Shell mount.
- **Risks** — containment, side effects, error handling, performance, and verification risks.

Ask targeted questions for missing items. Record the answers in a brief. Do not represent bespoke behavior as deterministically generated.

## Start with the Starter

For a normal single package, after the brief, run:
```sh
resonate package create <lowercase-kebab-id>
```

The command creates only `src/packages/<id>` under the Resonance application root, lists its files, runs its focused test, and prints a manifest snippet. It must return before installation/config loading/server startup/browser launch. Do not skip the focused test or replace it with a Docs package.

After reviewing the output, the developer—not the command—may add the printed `"<id>": { "module": "src/packages/<id>/index.ts" }` entry to the viewed repository’s authoritative package allowlist. The starter never edits that manifest.

## Hand Off Bespoke Work

Use the brief and generated package as inputs to normal design and implementation work. The scaffold is deterministic only for the golden package shape; design and implement custom data, routes, inputs, rendering, and tests separately.

Read `docs/architecture.md`, the relevant package README, `src/package-contract.ts`, and colocated tests before changing behavior. Keep Shell’s browser frame and Docs’ Markdown discovery out of the package. Register only namespaced `/api/<id>/...` routes and `/assets/<id>/...` assets through the shared contract; browser code renders only in Shell’s supplied mount. Resolve every repository file with `HostContext.resolveRepositoryPath()` before reading it. Keep handlers read-only unless a separate requirement explicitly authorizes writes.

Add colocated tests for registration/input validation, route/error behavior, containment, and browser lifecycle. Run the focused test, then `bun test` before presenting work.

## Sharing This Skill

This canonical skill is `.agents/skills/package-authoring/SKILL.md`. Pi discovers it from the working directory or its ancestors after trust. When the packaged skill is outside the viewed repository ancestry, use a repeatable explicit `--skill <path>` or a Pi settings `skills` entry. Do not claim cross-repository auto-discovery and do not add a skill/agent runtime manager.
