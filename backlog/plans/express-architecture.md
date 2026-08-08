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

- **Explain skill** — Describe entities, relationships, and views from the canonical model. Answer "what is X?", "how does Y relate to Z?", "show me the deployment view." The skill requires the agent to read the canonical model and requested view first, preserve relationship direction, distinguish modeled intent from implementation evidence and assessment, and report missing facts instead of inventing them.
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

1. 🟢 **Enable the Architecture package in `.resonance/config.json`.**
   → The package is already wired in the default config with module, artifactRoot, provider, and model settings.

2. 🟢 **Open the workspace and see the left-hand navigator with the system context, container, component, and deployment views.**
   → The navigator now includes the committed LikeC4 deployment topology view, showing the local runtime, package instances, target repository, and browser connections.

3. 🟢 **Click any view and see a rendered, layouted C4 diagram in the central pane.**
   → The LikeC4 renderer is integrated and functional — `ReactLikeC4` renders layouted diagrams from the LikeC4 dump.

4. 🟢 **Click an entity in the diagram and see its description, relationships, and linked evidence.**
   → The workspace details panel surfaces the selected entity's name, description, technology, modeled relationships, and linked evidence, including evidence from related relationships.

5. 🟢 **Open the validation panel and see `pass`/`fail`/`unknown` results for the implemented checkers.**
   → Six checkers (authoritative-config, shell, ownership, routes, containment, git) are fully wired — server API, UI button, result rendering with color-coded statuses, and CSS styling are all present and functional.

6. 🟢 **Ask the Architecture agent "explain the system context" and receive a coherent explanation grounded in the canonical model.**
   → The dedicated `explain` skill is mounted alongside `likec4-dsl` and `code-structural-view`. It directs the agent to call `read_model`, select and call `read_view` for the system-context view, use `read_entity` and linked `read_evidence` when needed, preserve relationship direction, and structure the response around scope, participants, relationships, boundaries, and grounding. It distinguishes modeled intent, implementation evidence, and agent assessment, and reports missing facts instead of inventing them.

### Legend

- 🟢 **Green** = Criterion is fully met.
- 🟡 **Yellow** = Criterion is partially met — functional but missing a key component (dedicated skill, UI panel, data, etc.).
- 🔴 **Red** = Criterion is not met at all.

### Note on relationship to Arch validation

The **Arch validation** decision (`backlog/plans/arch-validation.md`) is a separate, complementary workstream. It repairs trust defects in the existing validation pipeline (checker dispatch, false passes, LikeC4 parsing path) and introduces the intended-vs-observed graph model with dependency-cruiser integration. Where this decision establishes the **scaffolding and content** (diagrams, model, checkers, agent, UI), Arch validation makes the **results trustworthy** and adds dependency-level analysis. Criteria 7 and 8 (agent review and authoring) have been moved to Arch validation; criterion 6 (agent explain) is now addressed by the dedicated explain skill.