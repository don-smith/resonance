# Express architecture

Owner: team

## Context

The Architecture package foundation was established earlier: the package structure, the UI workspace with the left-hand LikeC4 navigator, the central `@likec4/diagram` renderer, the collapsible Architecture agent panel, and the agent chat interface are all in place. Those pieces proved the architecture workspace concept.

This decision is the hands-on build-out that makes the architecture package deliverable and usable in practice. It takes the established scaffolding and fills it with real content: diagrams, C4 models, validation rules, agent skills, documentation, and the integration wiring that connects everything together.

## What's already done

- Architecture package structure and scaffolding
- LikeC4 integration with the `@likec4/diagram` React renderer
- UI workspace with left-hand navigator and collapsible agent panel
- Agent chat interface and basic agent wiring
- Left-hand listing of architecture views

## Scope of this work

### 1. C4 model and diagrams

Author and commit the canonical LikeC4 model of Resonance's architecture:

- **System context diagram** — Resonance as a system, its users (developers), and its external dependencies (Langfuse, filesystem, Git, browser).
- **Container diagram** — Server, Host, Packages (Backlog, Telemetry, Architecture, etc.), Shell, Workspace UI, and the relationships between them.
- **Component diagrams** — Key internal components: the Backlog agent lifecycle, the package contract system, the telemetry pipeline, and the Shell capability model.
- **Deployment views** — How Resonance runs in development (local Bun process), the file layout, and the configuration surface.

Each view must be hand-authored LikeC4 source committed in `architecture/`, not generated or inferred. Diagrams are projections of the canonical model, not independent sources of truth.

### 2. Validation rules and checkers

Implement the deterministic checkers that validate architecture assertions against the running codebase:

- **Package ownership rules** — Assert that routes, assets, and commands are namespaced to their owning package.
- **Route and asset namespace rules** — Verify no cross-package routing leaks.
- **Shell role rules** — Validate that Shell's required capabilities match the packages that depend on it.
- **Repository containment rules** — Check that all package state stays within the repository root.
- **Package configuration rules** — Validate that package manifests are authoritative and complete.

Wire these into the workspace collapsible context/validation panel so results are visible as `pass`, `fail`, or `unknown`.

### 3. Architecture agent skills

Build out the Architecture agent's domain skills:

- **Explain skill** — Describe entities, relationships, and views from the canonical model. Answer "what is X?", "how does Y relate to Z?", "show me the deployment view."
- **Authoring skill** — Create and edit views, diagrams, and model elements through conversation, with schema validation and stable IDs.
- **Review skill** — Perform architecture reviews using repository-wide read-only inspection. Distinguish verified conformance, verified violations, unresolved questions, and qualitative agent assessment.
- **Validation skill** — Run checkers on demand, explain results, and suggest fixes for violations.

The agent's domain tools enforce schema validation, repository containment, stable IDs, serialized atomic writes, and stale-write detection. The agent receives a package-owned read-only filesystem backend (listing, reading, globbing, grep; no writes, shell, or network) for evidence gathering.

### 4. Documentation and integration

- Create or update `docs/architecture.md` to explain the architecture workspace, how to use the C4 diagrams, how to run validation, and how to interact with the Architecture agent.
- Wire the package into the default `.resonance/config.json` example so it's easy to enable.
- Ensure the workspace contributes namespaced routes and assets through the shared package contract.
- Add fixture coverage for the hand-authored LikeC4 model (at minimum the system context and container views).

### 5. Testing and verification

- Unit-test the checker interface with known pass/fail/unknown cases.
- Unit-test the agent domain tools (schema validation, containment, stale-write detection).
- Assert that the LikeC4 model parses, validates, and layouts without errors.
- Assert that the workspace renders the navigator, diagram, and panels without runtime errors.
- Verify review output distinguishes verified conformance, verified violations, unresolved questions, and agent assessment.
- Run `bun test` after implementation.

## Non-goals

- Automatic architecture discovery or inference from implementation code.
- Arbitrary freeform drawing as canonical data.
- Raw repository-authored SVG/HTML as diagram sources.
- Direct modification of implementation code through the agent.
- Persisted review results treated as authoritative state.
- A CLI/CI adapter for enforcing rules outside the workspace.

## Completion criteria

This decision is complete when a developer can:

1. Enable the Architecture package in `.resonance/config.json`.
2. Open the workspace and see the left-hand navigator with the system context, container, component, and deployment views.
3. Click any view and see a rendered, layouted C4 diagram in the central pane.
4. Click an entity in the diagram and see its description, relationships, and linked evidence.
5. Open the validation panel and see `pass`/`fail`/`unknown` results for the implemented checkers.
6. Ask the Architecture agent "explain the system context" and receive a coherent explanation grounded in the canonical model.
7. Ask the Architecture agent to review package ownership and see a structured report with distinct findings.
8. Propose a model change through the agent and have it applied with validation.