# Architecture

Resonance is a local, manifest-driven workspace. The CLI reads the current working directory as the authoritative repository root, offers `resonate install` when `.resonance/config.json` is absent, and starts a localhost HTTP server only after a configuration exists. Installation always writes Shell and interactively selects Home and Docs; the configured package entries are an authoritative allowlist. The server serves the Shell document and package assets, then dispatches package-owned API routes under `/api/<package-id>/...`.

Packages are Resonance's general extensibility and implementation units. A workspace is a user-visible surface mounted and navigated by Shell. A package may provide a workspace, but not every package must be a workspace; Shell is a required infrastructure package, not a workspace. Current Home, Docs, Backlog, and Pi Agent packages provide workspaces. This relationship is descriptive for now and does not add workspace metadata or a second configuration model.

## Package boundaries

- [**Shell**](../src/packages/shell/README.md) owns the fixed browser bootstrap, primary navigation, workspace mounts, activation rollback, and shared layout.
- [**Home**](../src/packages/home/README.md) reads the configured repository landing source and renders the Home workspace in the browser.
- [**Docs**](../src/packages/docs/README.md) discovers Markdown beneath `HostContext.repositoryRoot`, exposes the tree/document routes, renders the Docs workspace whenever Docs is activated, and navigates relative links between discovered documents.
- [**Pi Agent**](../src/packages/pi-agent/README.md) owns the developer-specific server-side Pi ACP session, prompt submission, incremental activity events, transcript, and local recovery controls for the Pi Agent workspace.

Packages contribute method-aware routes, assets, navigation metadata, and one browser module through the shared package contract. The host validates contributions transactionally, keys routes by `METHOD pathname`, and keeps optional package failures isolated. Route handlers receive package-safe request/response capabilities: bounded JSON parsing, JSON responses, request-abort notifications, and SSE streams. They do not receive Node HTTP objects. Package cleanup callbacks run in reverse registration order when the HTTP server closes; disposal is idempotent.

## Transport and safety boundaries

The shared HTTP layer bounds JSON request bodies and provides SSE framing. The server preserves `405` responses and lists registered methods in `Allow`. SSE handlers do not attempt a JSON response after headers are committed.

Filesystem access remains server-owned. Resonance remains localhost-only.
