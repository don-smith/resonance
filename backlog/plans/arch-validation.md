# Arch validation

Owner: team

## Context

The Architecture package has a validation system that reports `pass`/`fail`/`unknown` results in the workspace collapsible panel. However, a research audit (`docs/research/architecture-validation.md`) identified eight trust defects that make the current validation output unreliable as architecture conformance evidence. The system reports green results without actually validating the canonical LikeC4 model, and several checkers can produce false passes or are unreachable.

This decision repairs those trust defects and operationalizes the hybrid authority model: **intended graph** (authored LikeC4 model + executable rules) compared against **observed graph** (facts extracted from the current codebase). Validation succeeds only when deterministic evidence supports the finding.

## Trust defects to fix

1. **Validation does not validate the canonical model.** The current `validateArchitecture()` path loads a legacy JSON projection, not `model.c4`. Invalid LikeC4 can coexist with six green validation rows.

2. **Validation revision excludes LikeC4.** `ArchitectureArtifacts.revision` hashes five JSON files only. Editing `model.c4` does not change the revision.

3. **"Shell is required" rule does not run its Shell checker.** Both the authoritative-config and Shell rules select `authoritative-config`. `checkShell()` exists but is not reachable by the rule schema or dispatcher.

4. **Six rules, not six working checker identities.** The schema permits five checker names. Dispatch is an `if` chain whose final branch runs the Git checker for anything not matched explicitly.

5. **Route/asset checker can produce false pass.** It applies regex to package entry files and passes when no violating literal is found. It does not establish that all contributions were found, follow imported declarations, evaluate template expressions, or inspect the registration the host accepted.

6. **Useful rules duplicate runtime invariants.** Manifest shape, Shell presence, contribution namespacing, and path containment are already enforced by config/host interfaces. They do not verify modeled coupling, dependency direction, patterns, data flows, or seams.

7. **Validation uses the compatibility projection, not the rendered architecture.** Package ownership and evidence checks use `model.json` while developers browse `model.c4`. The two can drift without a finding.

8. **Coverage tests do not challenge each checker.** The current test establishes valid statuses and one positive result. It does not mutate one architectural fact per checker and assert expected failure.

## Scope of this work

### 1. Repair the existing validation pipeline

- Make `validateArchitecture()` parse `model.c4` via the LikeC4 first-party interface and incorporate LikeC4 parser/reference errors into the reported findings.
- Include `model.c4` in the validation revision hash so that editing the canonical model invalidates cached results.
- Fix the checker dispatch: replace the `if` chain with an explicit checker registry that maps schema names to checker functions, so every route maps to exactly one checker and unknown names produce a recognizable error rather than silently falling through.
- Wire `checkShell()` into the registry so the Shell rule actually runs the Shell checker.
- Fix the route/asset checker to require positive evidence: assert that declared contributions were found and that the host's registration was inspected, rather than passing on absence of regex matches.
- Align the validation read path with the rendered architecture — use LikeC4's model traversal for evidence queries rather than the legacy JSON projection.

### 2. Introduce the intended-vs-observed graph model

- **Intended graph** — the LikeC4 model (elements, relationships, metadata with source bindings and verification classifications) plus executable rules expressed as code.
- **Observed graph** — facts extracted from the current codebase: package manifests, directory layout, TypeScript dependency graph, and selected symbols.
- Define a comparison interface: an intended dependency that is absent from the observed graph is a failure; an observed dependency that is forbidden by the intended graph is a failure; an authored relationship that has no deterministic evidence is an assertion (displayed as `unknown`), not a pass.
- Do not change the result vocabulary: `pass`, `fail`, `unknown` remain, but `unknown` now means "no deterministic evidence" rather than "checker not implemented."

### 3. Adopt dependency-cruiser for TypeScript dependency analysis

- Add `dependency-cruiser` as a package-owned adapter behind the observed-graph abstraction.
- Configure it to extract dependencies at the package/module seam level (not every function).
- Implement the first wave of dependency-oriented checks:
  - Forbidden cross-package imports (packages must not import from sibling packages they do not declare as dependencies).
  - Forbidden package-to-host imports (packages must not import host internals directly).
  - Cycles between architectural modules.
  - Selected required dependencies (intended coupling that must be present in the observed graph).
- Use `dependency-cruiser`'s programmatic `cruise()` interface and structured output.
- Do not replace with the TypeScript Compiler API yet; reserve it for future symbol-level checks.

### 4. Add LikeC4 metadata-based source bindings

- Support reading `metadata.source`, `metadata.symbol`, `metadata.declaration_kind` from LikeC4 elements and relationships.
- Support reading `metadata.verification` and `metadata.required` from relationships.
- Coarse bindings first (container level); unbound elements display as authored/unverified, not as failures.
- Do not count unbound elements as verified.

### 5. Rewrite the checker test suite

- For each checker, mutate one architectural fact and assert the expected failure.
- Cover the Shell dispatch fix, the route/asset positive-evidence requirement, and the LikeC4 parsing path.
- Assert that the LikeC4 model parses, validates, and layouts without errors as part of the test suite.
- Run `bun test` after implementation.

### 6. Documentation

- Update `docs/architecture.md` to explain the intended-vs-observed graph model, how to interpret `unknown` results, and how to add source bindings via LikeC4 metadata.
- Document the checker registry pattern so new checkers can be added without modifying the dispatch logic.

### 7. Agent review skill

- Implement a dedicated review skill that produces a structured report distinguishing verified conformance, verified violations, unresolved questions, and qualitative agent assessment.
- The agent runs checkers on demand, explains results, and suggests fixes for violations.

### 8. Agent authoring skill

- Implement a dedicated authoring skill for creating and editing views, diagrams, and model elements through conversation.
- Enforce schema validation, stable IDs, serialized atomic writes, and stale-write detection.

## Non-goals

- Automatic architecture discovery or inference from implementation code (static analysis extracts facts, not architecture).
- Replacing the LikeC4 model as the source of truth for intended architecture.
- Symbol-level (function/class) dependency analysis beyond what dependency-cruiser provides out of the box.
- A CLI/CI adapter for enforcing rules outside the workspace.
- Persisted validation results treated as authoritative state.
- Adding new checker types beyond the repair and dependency-cruiser integration described here.

## Relationship to "Express architecture"

The Express architecture decision (backlog/plans/express-architecture.md) defines the hands-on build-out of the Architecture package, including validation rules and checkers in scope item 2. This decision repairs the trust defects discovered in the existing validation pipeline and introduces the intended-vs-observed graph model. The two decisions are complementary: Express architecture builds the checker scaffolding; this decision makes the results trustworthy.

Criteria 7 and 8 (agent review and authoring skills) were moved from Express architecture to this decision, as they are not yet started and fit naturally alongside the other agent-facing improvements in this workstream.

## Completion criteria

This decision is complete when:

1. Editing `model.c4` changes the validation revision, and LikeC4 parser/reference errors appear as findings in the validation panel.
2. The Shell rule runs `checkShell()` and produces a distinct result from the authoritative-config rule.
3. Every checker name in the schema maps to exactly one checker function; unknown names produce a recognizable error.
4. The route/asset checker requires positive evidence (contribution found, host registration inspected) and does not pass on absence of regex matches.
5. The validation read path uses the LikeC4 model traversal, not the legacy JSON projection.
6. `dependency-cruiser` is integrated and produces observed-graph facts for the first wave of checks (forbidden cross-package imports, forbidden package-to-host imports, cycles, required dependencies).
7. 🔴 **Ask the Architecture agent to review package ownership and see a structured report with distinct findings.**
   → The agent has `validate_architecture` which runs the ownership checker. **There is no dedicated "review" skill** that produces a structured report distinguishing verified conformance, verified violations, unresolved questions, and qualitative assessment. The agent can run validation, but the output format isn't enforced.
8. 🔴 **Propose a model change through the agent and have it applied with validation.**
   → The agent has write_file/edit_file capabilities for architecture files, and the system prompt instructs it to repair model issues. LikeC4 parse errors are returned as recoverable context. **There is no dedicated "authoring" skill** with schema validation and stable ID enforcement. The agent can do it, but with higher risk of inconsistent edits.
9. LikeC4 metadata source bindings are read and displayed; unbound elements are shown as authored/unverified, not as failures.
10. The checker test suite mutates one fact per checker and asserts the expected failure.
11. `bun test` passes.
12. Documentation explains the intended-vs-observed graph model and how to interpret `unknown` results.