# Architecture package

Owner: team

## Decision

Build a team-owned Architecture package that serves existing developers and agents. It provides a workspace and an Architecture agent for describing, exploring, validating, and reviewing Resonance's architecture without changing implementation code.

The package uses a hybrid authority model:

- Implementation-derived facts are authoritative for what exists.
- Repository-authored architecture assertions are authoritative for intended structure, rules, patterns, and decisions.
- Diagrams and other views are projections of those facts and assertions, never independent sources of truth.

The core must remain useful without an LLM or a polished visualization renderer.

## Architecture artifacts

Committed LikeC4 source files live in a top-level `architecture/` directory, separate from `.resonance/` runtime configuration. The `.c4`/`.likec4` files are the canonical text model and view language; `rules.json`, `patterns.json`, and `decisions.json` retain package-owned validation and decision metadata.

LikeC4 owns the architecture vocabulary, typed elements, relationships, nested containers/components, view predicates, and layouted diagram model. Resonance adds bounded evidence and deterministic validation around that model rather than maintaining a competing graph schema.

Existing `docs/architecture.md` remains explanatory documentation initially. The package may link its concepts to canonical entities, but it does not duplicate the document into the model or make Markdown the validation authority.

## Workspace and views

The workspace provides a left-hand LikeC4 view navigator, a central `@likec4/diagram` renderer, a collapsible bottom context/validation panel, and a collapsible right-side Architecture agent. LikeC4 supplies pan, scroll-wheel zoom, selection, navigation, layout, accessible diagram semantics, and relationship details. The architecture package does not duplicate LikeC4's view/query/presentation language.

## Validation

The package owns an internal checker interface and deterministic, bounded checkers for local implementation evidence such as manifests, source imports, routes, assets, tests, and Git revisions. Checkers do not require network access or unrestricted agent shell access. A shared host contract is deferred until another package needs the capability.

Each rule declares its applicable entities, constraints, evidence checker, and severity. Results are explicitly `pass`, `fail`, or `unknown`; agent assertions alone cannot produce `pass`. The workspace displays all results. A future CLI/CI adapter may enforce selected rules and fail checks, but Resonance startup is not blocked by a failed or unknown rule.

The first checks dogfood Resonance's load-bearing decisions: package ownership, route and asset namespacing, Shell's required role, repository containment, and authoritative package configuration.

## Agent and mutation boundaries

One Architecture agent provides explanation, authoring, and review skills over the same model and checker interface. Its Architecture domain tools enforce schema validation, repository containment, stable IDs, serialized and atomic writes, and stale-write detection. For assessment, the agent also receives a package-owned Deep Agents filesystem backend with repository-wide read-only access, excluding credential files such as `.env` and repository-local agent credential files; listing, reading, globbing, and grep are available, while writes, edits, shell execution, traversal, and symlink escapes are rejected.

The agent may create and edit views, diagrams, explanations, rules, and decision metadata in conversation with the human. Deletions and changes to rules, decisions, or validation-affecting relationships require explicit confirmation. Implementation code is never modified. Review output distinguishes verified conformance, verified violations, unresolved questions, and qualitative agent assessment.

Review results are ephemeral by default. An explicitly saved report may record the Git revision, checker identities and versions, findings, and evidence, but an old report is never treated as current truth. The agent runtime is package-owned and lazy; it may reuse proven runtime mechanics without importing Backlog's domain module.

## Package integration

Architecture is an optional team package selected explicitly in `.resonance/config.json`; it is never loaded implicitly. Its default artifact root is `architecture/`, with only a narrow package-specific root override if needed. It contributes namespaced routes and assets through the shared package contract and does not depend directly on Docs. Source evidence is exposed through Architecture-owned metadata and a narrow lookup surface, with optional Docs links.

## Delivery sequence

1. Adopt and validate the LikeC4 source model and view language.
2. Add a small hand-authored LikeC4 model of Resonance and fixture coverage.
3. Expose the validated, layouted LikeC4 model through namespaced routes.
4. Integrate the official `@likec4/diagram` React renderer.
5. Implement the deterministic checkers and context/validation UI.
6. Add the Architecture agent and review skill.

V1 does not include exhaustive automatic architecture discovery, arbitrary freeform drawing as canonical data, raw repository-authored SVG/HTML, direct implementation edits, or persisted review results treated as authoritative state. LikeC4 source and its generated layouted model remain the diagram authority.

## Completion criteria

The decision is complete when a developer can configure Architecture, open the workspace, navigate the initial Resonance views, inspect entities and evidence, run validation and see `pass`/`fail`/`unknown` results, and safely edit committed architecture artifacts. The agent can explain findings, perform architecture reviews using repository-wide read-only inspection, and propose or apply confirmed model, rule, decision, and view changes without implementation writes, shell, network, or arbitrary persistence access.

