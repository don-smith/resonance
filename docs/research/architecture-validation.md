# Architecture validation: connecting the model to code

## Question

How should Resonance turn its C4 model into a trustworthy, low-maintenance notification system for unexpected architectural change, while retaining deterministic evidence and using an agent only where static analysis cannot establish the answer?

## Recommendation

Keep the hybrid authority decision, but make it operational with **two explicit graphs**:

1. **Intended graph** — the authored LikeC4 model and executable rules.
2. **Observed graph** — facts extracted from the current manifest, filesystem, TypeScript dependency graph, and selected symbols.

Validation should compare those graphs. An observed dependency forbidden by the intended graph is a failure. A required intended dependency absent from the observed graph is a failure. An authored relationship that has no deterministic evidence is an assertion, not a pass. This preserves the existing decision that implementation establishes what exists while authored architecture establishes what is intended.

Start at package/module seams, not at every function in the Level 4 diagrams. The first valuable checks are forbidden cross-package imports, forbidden package-to-host imports, cycles between architectural modules, model source bindings, and selected required dependencies. These are likely to catch accidental mixing while remaining stable during ordinary refactoring.

Use `dependency-cruiser` behind a package-owned adapter for the first TypeScript dependency graph. It supports TypeScript, type-only dependencies, circular checks, forbidden/allowed/required rules, structured output, and a programmatic interface. Add the TypeScript Compiler API only when symbol-level checks justify the added interface and configuration. Do not use Bun's import scanner as the trusted graph: Bun documents `scanImports()` as marginally less accurate, and a local check showed that pure `import type` dependencies are omitted.

The agent should be a **drift scout and rule authoring assistant**, never an evidence provider capable of turning a result green. It can explain deterministic findings, inspect relationships that remain unknown, and propose a model binding or a new executable rule.

## Current-state audit

The foundations are useful:

- LikeC4 is the canonical diagram language and the store already rejects LikeC4 parser/reference errors.
- The result vocabulary (`pass`, `fail`, `unknown`) is appropriate.
- Validation is package-owned, bounded, and does not block startup.
- The architecture plan already states the right authority model and says agent assertions alone cannot produce `pass`.

The current validation result is not yet trustworthy as architecture conformance:

1. **Validation does not validate the canonical model.** `GET /api/architecture/validation` calls `store.read()` and `validateArchitecture()`. `read()` loads the legacy JSON projection and metadata but not `model.c4`; LikeC4 parsing happens through a separate `likec4()` path. Invalid LikeC4 can therefore coexist with six green validation rows. See [`index.ts`](../../src/packages/architecture/index.ts), [`architecture-store.ts`](../../src/packages/architecture/architecture-store.ts), and [`architecture-checkers.ts`](../../src/packages/architecture/architecture-checkers.ts).

2. **The reported validation revision excludes LikeC4.** `ArchitectureArtifacts.revision` hashes the five JSON files only. Editing `model.c4` does not change the validation revision.

3. **The “Shell is required” rule does not run its Shell checker.** Both the authoritative-config and Shell rules select `authoritative-config`. `checkShell()` exists but is not selectable by the rule schema or dispatcher, so the Shell row repeats the manifest-version result and message.

4. **There are six rules, not six working checker identities.** The schema permits five checker names. Dispatch is an `if` chain whose final branch runs the Git checker for anything not matched explicitly. Schema validation currently limits that risk, but the dispatch interface makes a wrong mapping fail open to an unrelated check.

5. **The route/asset checker can produce a false pass.** It applies regular expressions to each package entry file and passes when it finds no violating literal. It does not establish that all contributions were found, follow imported declarations, evaluate template expressions, or inspect the registration that the host actually accepted. “No regex match” is not positive evidence.

6. **The useful rules duplicate runtime invariants more than they validate architecture.** Manifest shape, Shell presence, contribution namespacing, and path containment are already enforced by the config/host interfaces. Reporting those facts can be useful, but it does not verify the modeled coupling, dependency directions, patterns, data flows, or seams.

7. **Validation uses the compatibility projection rather than the rendered architecture.** Package ownership and evidence checks use `model.json`, while developers browse the much richer `model.c4`. The two can drift without a finding. `patterns.json` and `decisions.json` remain descriptive, as the package README correctly states.

8. **Coverage tests do not challenge each checker.** The current test establishes valid statuses and one positive config result. It does not mutate one architectural fact per checker and assert the expected failure. The Shell dispatch defect and zero-evidence route pass consequently remain undetected.

These are trust defects rather than a need for more checker count. Adding semantic checks before repairing them would make the green screen look stronger without making it more reliable.

## Evidence from existing tools

### LikeC4

LikeC4 already supplies the intended-graph primitives Resonance needs. Its first-party package interface parses workspaces, reports source errors with `getErrors()`, and exposes model traversal over elements and relationships. Current LikeC4 also supports typed metadata values on elements and relationships; its repository examples use metadata in model/view queries. Resonance can therefore place machine-readable source bindings and verification classifications in `model.c4` instead of creating a second graph.

Example direction:

```c4
host = container "Host registry" {
  metadata {
    source "src/host.ts"
    architecture_boundary "host"
  }

  createHostFn = code "createHost()" {
    metadata {
      source "src/host.ts"
      symbol "createHost"
      declaration_kind "function"
    }
  }
}

host -> config "Reads configuration" {
  metadata {
    verification "static-dependency"
    required true
  }
}
```

Keep `link` for human navigation; use metadata for fields whose shape checkers depend on. Bind coarse elements first. Unbound code-level elements should be displayed as authored/unverified, not treated as failures and not counted as verified.

Sources:

- [LikeC4 package: workspace parsing, errors, and model traversal](https://github.com/likec4/likec4/blob/v1.59.2/packages/likec4/README.md)
- [LikeC4 first-party metadata example](https://github.com/likec4/likec4/blob/main/examples/metadata-views/views.c4)

### Dependency Cruiser

Dependency Cruiser is a close TypeScript analogue of the dependency-oriented parts of ArchUnit. Its first-party documentation supports:

- JavaScript and TypeScript dependency extraction and validation;
- forbidden, allowed, and required dependency rules;
- direct and reachable dependencies;
- cycles and type-only dependencies;
- structured reports and a programmatic `cruise()` interface;
- a known-violations baseline for incremental adoption.

The programmatic interface matters because Architecture checkers should not require shell access. Resonance should consume the graph behind a narrow internal adapter and translate findings into its own stable result format rather than exposing Dependency Cruiser's configuration as the Architecture package interface.

A baseline is useful only for migration. Known violations should stay visible as accepted debt; they should not become architectural passes.

Sources:

- [Dependency Cruiser overview](https://github.com/sverweij/dependency-cruiser/blob/main/README.md)
- [Rule reference](https://github.com/sverweij/dependency-cruiser/blob/main/doc/rules-reference.md)
- [Programmatic interface](https://github.com/sverweij/dependency-cruiser/blob/main/doc/api.md)
- [Known-violation baseline](https://github.com/sverweij/dependency-cruiser/blob/main/doc/cli.md#--ignore-known-ignore-known-violations)

### ArchUnit

ArchUnit is Java-specific and should not be added to Resonance, but its first-party rule library demonstrates the useful fitness-function shapes: package dependency rules, layered/onion dependency direction, cycle-free slices, and checking code dependencies against an authored component diagram. Those are better templates for Resonance than attempting to prove every descriptive C4 arrow.

Sources:

- [ArchUnit examples of package and class dependency checks](https://github.com/TNG/ArchUnit/blob/main/docs/userguide/004_What_to_Check.adoc)
- [ArchUnit architecture, cycle, and diagram-backed rules](https://github.com/TNG/ArchUnit/blob/main/docs/userguide/008_The_Library_API.adoc)

### TypeScript and Bun

The TypeScript Compiler API exposes a project `Program`, `SourceFile` ASTs, and compiler diagnostics. It is the appropriate later tool for verifying that a named function/interface exists, resolving a call to a symbol, or checking that a factory parameter uses an intended port type. That work requires a direct `typescript` dependency and an explicit project configuration; Resonance currently has neither, so it should not claim type-checking confidence yet.

Bun's `Transpiler.scanImports()` is attractive for a preview because Bun is already required, but Bun explicitly describes it as faster and marginally less accurate. It also omits erased pure type imports, which are still architectural coupling. It should not be the source of a trusted green result.

Sources:

- [TypeScript Compiler API](https://github.com/microsoft/TypeScript-wiki/blob/main/Using-the-Compiler-API.md)
- [Bun Transpiler and `scanImports()`](https://bun.com/docs/runtime/transpiler#scanimports)

## Proposed validation module

Keep one deep external interface:

```ts
type ArchitectureValidator = {
  validate(): Promise<ArchitectureValidation>
}
```

The package route, browser, agent tool, and future CLI should all cross this same seam. The implementation can have internal adapters for LikeC4, code dependencies, source symbols, repository files, and Git identity, but callers should not need to orchestrate checkers or know which analyzer produced the observed graph.

A validation run should build one immutable snapshot:

```ts
type ArchitectureSnapshot = {
  intended: LikeC4Model
  observed: CodeDependencyGraph
  modelRevision: string
  sourceRevision: string
  coverage: ArchitectureCoverage
}
```

Each finding should include the rule, status, checker/version, current revisions, concrete evidence locations, and a reason when status is `unknown`. “Checker read zero applicable files,” “element has no binding,” and “analyzer could not resolve import” must be unknown, never pass.

Do not build a generic architecture-rule programming language initially. Add a small discriminated set of rule shapes that correspond to demonstrated needs:

- model is valid;
- element binding exists;
- dependency is forbidden;
- dependency is required;
- architectural slice is acyclic;
- concrete construction is confined to a composition root (later, symbol-aware).

This keeps checker behavior local and testable while `rules.json` remains the authored policy list. `appliesTo` should reference LikeC4 FQNs/selectors, and `constraints` should become schema-validated input for the selected rule shape rather than reserved, ignored data.

## Highest-value first rules for Resonance

1. **No package implementation imports another package implementation.** Shared behavior must cross an approved shared module or the package contract, not `src/packages/<other-id>/...`.
2. **Package-to-host dependencies use an explicit allowlist.** Start with `package-contract.ts` and deliberately approved shared Markdown/content modules. Imports of `host.ts`, `server.ts`, `http.ts`, config/member loaders, or state implementation from a package should fail unless a decision explicitly permits one.
3. **Package code does not depend on Node HTTP objects.** This directly protects the package-safe request/response seam.
4. **No cycles between top-level runtime modules and package modules.** Report type-only and runtime cycles distinctly, but count both as architectural coupling.
5. **Bound LikeC4 sources exist and belong to the declared seam.** A package/container bound to `src/packages/docs/**` must not silently move into another package.
6. **Selected modeled static dependencies match observed imports.** Start only with container/component relationships tagged `verification = "static-dependency"`; do not reinterpret HTTP, SSE, data-flow, ownership, or human interaction arrows as imports.
7. **Optional package implementations remain behind the configuration/composition seam.** Prefer an observed dependency rule over a brittle textual check for “dependency injection.”

Dependency injection is a means, not a universal architectural property. Validate its intended outcome: core modules depend on interfaces/ports, concrete adapters are created only in declared composition roots, and package implementations are not imported across seams. Later symbol checks can verify selected factory parameter types and concrete construction sites. A broad rule such as “all dependencies must be injected” would be noisy and would discourage harmless local construction.

## Role of the agent

The agent is useful where a relationship is semantic rather than syntactic:

- Does a data flow description still match the implementation?
- Has responsibility moved even though imports did not?
- Is a new dependency intentional, and which rule or model relationship should record it?
- Does repeated unknown evidence justify a new deterministic checker?

Agent output should be labeled **assessment**, carry source evidence, and remain separate from deterministic status. It may propose edits to model metadata, rules, patterns, decisions, or documentation. Only rerunning deterministic validation can produce a pass.

A good improvement loop is:

1. deterministic checker reports fail or unknown;
2. agent explains and proposes intent/evidence changes;
3. human accepts an implementation or architecture change;
4. deterministic checker reruns;
5. recurring qualitative findings are promoted into a bounded checker.

## Delivery sequence

### Slice 0 — repair trust

- Include LikeC4 parsing and all `.c4` sources in validation and its revision.
- Replace checker dispatch fallback with an exhaustive registry and give Shell its own checker identity.
- Remove or downgrade checks that pass without positive evidence, especially regex route discovery.
- Add one mutation test per checker, including zero-evidence and analyzer-error cases.
- Show verified, failed, and unverified counts; do not present six green rows as overall conformance.

### Slice 1 — package/module fitness functions

- Add Dependency Cruiser as a direct dependency behind an internal observed-graph adapter.
- Add coarse LikeC4 source/boundary metadata.
- Implement the first five dependency/binding rules above.
- Add fixture mutations for regular imports, pure type imports, dynamic imports, cycles, and an intentional model/rule update.
- Display unexpected observed edges with source and target files.

This is the smallest slice likely to catch the drift described in the question.

### Slice 2 — model/code correspondence

- Classify selected LikeC4 relationships by verification method.
- Compare selected static relationships against the observed graph in both directions.
- Add explicit coverage: bound elements, deterministically verified relationships, authored-only relationships, and unsupported relationships.
- Stop using `model.json` as validation authority; derive compatibility projections or retire them after migration.

### Slice 3 — symbol checks and DI outcomes

- Add a direct TypeScript dependency and project configuration.
- Validate selected source symbols and declaration kinds.
- Add only load-bearing call/construction/port rules, especially composition-root confinement.
- Keep detailed Level 4 diagrams authored-only until individual relationships are worth the maintenance cost.

### Slice 4 — notifications and automation

- Expose the same validator through a CLI/CI command once its false-positive rate is acceptable.
- Fail CI only for selected error-severity deterministic violations; always report unknown and coverage regressions.
- In the workspace, compare the current result fingerprint with the previous run to highlight new/resolved findings, while marking prior results stale and never treating them as current truth.
- Give a positive signal such as: “12 deterministic rules passed; 0 failed; 4 relationships remain authored-only; model and code analyzed at these revisions.”

## Guardrails against over-architecture

- Validate a decision only when violating it would create meaningful cost or confusion.
- Prefer coarse stable seams over complete function call graphs.
- Require every rule to have a fixture that demonstrates a plausible bad change it catches.
- Require a clear remediation message and concrete source evidence.
- Track verification coverage separately from conformance; never turn missing coverage into a green result.
- Delete duplicated checks when an existing runtime interface already proves the same fact and Architecture adds no better evidence.
- Do not add a symbol analyzer, generic rule DSL, persisted review database, or continuous agent review until the package dependency slice proves useful.

## Bottom line

The current direction is still sound. The missing piece is not a smarter agent or a larger list of bespoke checkers; it is a trustworthy intended-versus-observed comparison at a few load-bearing seams. Repair false passes, make LikeC4 itself part of the validation snapshot, extract a real TypeScript dependency graph, and validate package/module dependency direction first. That will provide both useful drift notifications and credible positive signals without attempting to mechanically prove every diagram statement.
