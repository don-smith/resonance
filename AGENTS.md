# AGENTS.md

## Purpose

Resonance is a local, manifest-driven workspace for browsing and operating a repository. The `resonate` CLI starts a localhost HTTP server that composes the configured packages around the viewed repository.

## Repository map

- `bin/resonate` — executable CLI entrypoint and argument parsing.
- `src/` — host, HTTP, configuration, Markdown/content, and package-contract code; tests are colocated as `*.test.ts`.
- `src/packages/shell/` — shared browser document, navigation, mounts, and bootstrap.
- `src/packages/home/` — configured repository landing page.
- `src/packages/docs/` — Markdown discovery, tree navigation, and document rendering.
- `src/packages/pi-agent/` — optional in-memory Pi ACP session and SSE event bridge.
- `scripts/` — local installation tooling.
- `docs/` — architecture and usage documentation.
- `test/fixtures/` — repository fixtures used by tests.
- `.resonance/config.json` — the viewed repository's authoritative package manifest.

Read the relevant package `README.md` and `docs/architecture.md` before changing package behavior or host/package boundaries.

## Commands

```sh
bun install       # install dependencies (Bun 1.3.13)
bun test          # run the full test suite
./bin/resonate --help
bun run resonate  # start Resonance for the current repository
resonate install  # create .resonance/config.json interactively
```

There is no declared lint, formatter, typecheck, or build script; do not assume one exists. Add or update the relevant colocated tests with every behavior change.

## Architecture rules

- Shell is required: it owns `/`, the browser bootstrap, navigation, and package mounts.
- Home, Docs, and Pi Agent must keep their responsibilities within their package boundaries.
- Register package routes, assets, navigation, and browser entries through the shared contract in `src/package-contract.ts`; routes are namespaced under `/api/<package-id>/...` and assets under `/assets/<package-id>/...`.
- Package configuration is an authoritative allowlist. Do not implicitly load omitted packages or revive legacy manifest behavior.
- Package modules are app-root-relative. Repository files must be resolved through `HostContext.resolveRepositoryPath`; preserve repository-root containment.
- Package handlers use the package-safe request/response capabilities, not Node HTTP objects. Preserve method-aware routing, `405`/`Allow` responses, bounded JSON parsing, request-abort handling, and SSE lifecycle behavior.
- Resonance binds to localhost by default; `--host` can change the bind address, but remote access is not an authenticated or supported product boundary. Pi Agent requires an installed `pi` executable and configured model credentials; it has no persistence or remote-access layer.
- Configured repository HTML is trusted local markup. Only point it at repository-owned files and scope page-specific selectors below a page root.

## Change workflow

1. Inspect the relevant host, package, contract, and tests before editing.
2. Keep changes focused and preserve the existing TypeScript/Bun style.
3. Add regression coverage beside the source under test.
4. Run `bun test` before reporting completion.
5. Avoid committing `node_modules/`, `.myflow/`, or unrelated generated/local files.
