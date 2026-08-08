---
name: package-authoring
description: Use when creating, scaffolding, configuring, or changing a Resonance package, including when choosing between a starter and bespoke package work.
---

# Author Resonance Packages

## Core Rule

A Resonance package is Resonance's general extensibility and implementation unit. Packages have one of two explicit scopes:

- Team packages live in the Resonance application root and are selected by the viewed repository’s checked-in `.resonance/config.json`.
- Member packages live in a member-package repository and are selected by the viewed repository’s ignored `.resonance/member-config.json`.

A package may provide a user-visible workspace mounted by Shell, but package and workspace are not synonyms. Never auto-load a package, add a package to a manifest on the developer’s behalf, or put member-package source below the viewed repository.

## Capture the Brief First

Before selecting a starter or invoking the CLI, capture:
- **Purpose** — the user problem and the package’s bounded responsibility.
- **Data ownership** — repository files/data it reads or writes, and the read-only boundary.
- **Route, configuration, and UI** — named API routes, package inputs, navigation, and what renders in the Shell mount when the package provides a workspace.
- **Risks** — containment, side effects, error handling, performance, and verification risks.

Ask targeted questions for missing items. Record the answers in a brief. Do not represent bespoke behavior as deterministically generated.

## Start with the Starter

For a team package, after the brief, run:
```sh
resonate package create <lowercase-kebab-id>
```

For a member package, initialize or enter the member repository, then run:
```sh
resonate member package create <lowercase-kebab-id>
```

Each command creates only `src/packages/<id>` in its owning repository, lists its files, runs its focused test, and prints the appropriate manifest snippet. It must return before installation/config loading/server startup/browser launch. Do not skip the focused test or replace the package with a Docs package.

After reviewing the output, the developer—not the command—may add the printed entry to the owning explicit manifest: `.resonance/config.json` for team packages or `member-packages.json` for member packages. Starters never edit either manifest.

## Hand Off Bespoke Work

Use the brief and generated package as inputs to normal design and implementation work. The scaffold is deterministic only for the golden package shape; design and implement custom data, routes, inputs, rendering, and tests separately.

Read `docs/architecture.md`, `docs/design-system.md` when changing workspace UI, the relevant package README, `src/package-contract.ts`, and colocated tests before changing behavior. Keep Shell’s browser frame and Docs’ Markdown discovery out of the package. Register only namespaced `/api/<id>/...` routes and `/assets/<id>/...` assets through the shared contract; browser code renders only in Shell’s supplied mount. Reuse the shared browser modules in `src/ui/` for common workspace behavior in team packages, while keeping domain state and agent adapters package-owned. Member packages must not import the viewed application root; use only shared UI distribution explicitly provided by the member-package contract. Resolve every repository file with `HostContext.resolveRepositoryPath()` before reading it. Keep handlers read-only unless a separate requirement explicitly authorizes writes.

Add colocated tests for registration/input validation, route/error behavior, containment, and browser lifecycle. Run `bun run build:browser` after changing bundled browser sources, then run the focused test and `bun test` before presenting work.

## Sharing This Skill

This canonical skill is `.agents/skills/package-authoring/SKILL.md`. `resonate member init <folder>` copies it into the new member repository so package authors have the same guidance there. Pi discovers it from the working directory or its ancestors after trust. When the packaged skill is outside the viewed repository ancestry, use a repeatable explicit `--skill <path>` or a Pi settings `skills` entry. Do not claim cross-repository auto-discovery and do not add a skill/agent runtime manager.
