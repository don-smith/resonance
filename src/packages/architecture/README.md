# Architecture package

The optional team-owned Architecture package projects committed C4 architecture artifacts into a navigable workspace and runs bounded local validation. Its agent can update architecture artifacts and Markdown documentation, but never implementation code, and it does not depend on Docs.

## Configuration

Add this explicit entry to the viewed repository's `.resonance/config.json`:

```json
"architecture": {
  "module": "src/packages/architecture/index.ts",
  "artifactRoot": "architecture",
  "provider": "openrouter",
  "model": "deepseek/deepseek-v4-flash"
}
```

`artifactRoot` defaults to the top-level `architecture/` directory and is limited to a narrow repository-relative directory.

## Artifacts

LikeC4 `.c4` and `.likec4` files under `artifactRoot` are the canonical architecture model and view language. The package parses them with `LikeC4.fromWorkspace()`, validates the source, computes layouts, and passes the resulting model dump to `@likec4/diagram` for rendering. `rules.json`, `patterns.json`, and `decisions.json` remain package-owned metadata for validation and architectural context; the legacy JSON model/view projections are retained only for that metadata and migration compatibility.

The initial model is intentionally small and hand-authored. `docs/architecture.md` remains explanatory documentation rather than validation input.

## What is standard and what is custom?

`model.c4` is the only architecture artifact that uses an external modeling language. It is written in LikeC4's `.c4` DSL, which uses C4 concepts such as systems, containers, components, actors, relationships, and views. LikeC4 is the language and tool here; this is not an official C4 JSON interchange schema. LikeC4 parses the file, validates its syntax and references, computes the layout, and supplies the model rendered by the browser. The C4-style model is therefore authored in LikeC4 format rather than in one of Resonance's JSON schemas.

The JSON files are Resonance-owned documents. Their top-level `version`, fields, limits, and allowed values are defined by Zod schemas in `architecture-store.ts`; they are not a C4 standard or a general-purpose architecture interchange format:

- `model.json` is the package's typed, evidence-bearing projection of architectural entities and relationships. It is still read for validation, evidence lookup, the agent, and compatibility routes, but it is not the source used to render the LikeC4 diagram.
- `views.json` is compatibility metadata for named query views and presentation information. The browser prefers the views declared in `model.c4`.
- `rules.json` is a list of local validation declarations. Its `checker` field selects code implemented in `architecture-checkers.ts`.
- `patterns.json` is currently descriptive metadata for named architectural patterns and their evidence.
- `decisions.json` is currently descriptive metadata for accepted or proposed architectural decisions and their evidence.

Patterns and decisions are therefore useful architectural context today, but they are not yet executable constraints.

## How the package works

1. The manifest loads the package and supplies its narrow `artifactRoot`.
2. `createArchitectureStore()` resolves every artifact through `HostContext`, checks repository containment, parses the JSON documents, and reads LikeC4 sources beneath the artifact root.
3. The model and graph routes return the validated LikeC4 dump, its views, or one projected graph. The browser renders that dump with `@likec4/diagram`; it does not render repository-authored HTML or SVG.
4. The validation route reads the JSON artifacts and calls `validateArchitecture()`. The validator evaluates each rule in order and returns `pass`, `fail`, or `unknown` with a message, checker name, severity, and evidence paths. Results do not block server startup.
5. Evidence routes expose only files explicitly linked from the model, patterns, or decisions, with bounded content and repository containment.
6. The optional Architecture agent reads the model, views, rules, patterns, decisions, evidence, and validation results through package-owned tools. Tool and artifact failures are returned as recoverable context instead of aborting the model turn, so the agent can inspect and repair the relevant artifact or explain the problem to the user. It also receives a repository-root virtual filesystem with `ls`, `read_file`, `glob`, and `grep` operations, so assessments can inspect implementation and documentation beyond explicitly linked evidence. `write_file` and `edit_file` are allowed for the configured architecture artifact root and Markdown documents anywhere in the repository; implementation files remain read-only. Credential files such as `.env` files and repository-local agent credential files are intentionally unavailable, as are `.git` writes, shell execution, and network access.

The browser's **Run validation** button currently calls `GET /api/architecture/validation`; it does not ask LikeC4 to infer architectural intent and it does not run a generic pattern engine.

When an agent turn completes, the browser reloads the current LikeC4 model and graph before repainting the active view. Navigating between views performs the same refresh, so agent edits do not require a page reload. The chat footer reports the latest provider-reported input context as a rounded `used / limit` value after a response; it remains blank until the first usage report and clears on **New Chat**. Browser-source changes require rebuilding the generated asset with `bun run build:architecture`.

## What validation checks today

The six initial rules are mostly workspace and package-boundary invariants rather than semantic C4 rules:

- **Authoritative package configuration** — `.resonance/config.json` is a version 1 manifest with a package allowlist.
- **Shell is required** — Shell is present and enabled in that manifest.
- **Configured package ownership** — every modeled package has a matching manifest entry and expected module path.
- **Namespaced package contributions** — package routes and assets use their package namespace.
- **Repository evidence is contained** — linked evidence paths pass through the host's repository-path boundary.
- **Reviews identify a Git revision** — `.git/HEAD` is available for review context.

A rule's `description`, `appliesTo`, and `severity` document intent and reporting. The executable behavior comes from its `checker`; `constraints` are reserved schema data and are not interpreted by the current checkers. Unknown means the checker could not establish the fact, not that the architecture failed.

This explains why the current validation screen does not yet show checks such as “every package uses the package contract,” “this dependency direction is allowed,” or “this pattern is implemented.” Those are the next layer of architecture validation: they require patterns or rules to become explicit, executable assertions over the LikeC4 model and implementation evidence rather than remaining descriptive JSON.

## Routes

- `GET /api/architecture/model` returns the validated LikeC4 model dump and view list
- `GET /api/architecture/views` returns compatibility metadata
- `GET /api/architecture/graph?view=...&filter=...` returns a LikeC4 layouted view`
- `GET /api/architecture/evidence?path=...` for model-linked, bounded evidence
- `GET /api/architecture/validation`
- `GET/POST /api/architecture/agent/{state,events,prompt,credential,reset}`
- `POST /api/architecture/edit` for schema-validated, revision-checked artifact replacement

Validation results are `pass`, `fail`, or `unknown`. Unknown and failed rules never block Resonance startup. The `POST /api/architecture/edit` route uses atomic replacement, stale revision detection, schema validation, and explicit confirmation for model, rule, and decision changes. Agent filesystem writes are also atomic and bounded to the configured artifact root or Markdown files, but are intentionally available for direct maintenance.

## Boundaries

Architecture source evidence routes are only exposed when linked from committed artifacts and are resolved through `HostContext`. The Architecture agent uses the configured OpenAI or OpenRouter model and the packaged `likec4-dsl` skill. In addition to the bounded Architecture tools, its Deep Agents backend mounts the viewed repository for codebase assessment, excluding credential files such as `.env` and repository-local agent credential files. It permits writes and edits only in the configured artifact root and Markdown documents, and rejects implementation writes, `.git` and credential writes, shell execution, path traversal, and symlink escapes. Its local API key is stored in gitignored `.resonance/architecture-agent.env`; LikeC4 owns the diagram interaction and layout, and the package does not expose raw repository-authored SVG/HTML or grant implementation-editing capability.
