# Architecture

Resonance is a local, manifest-driven workspace. The CLI reads the current working directory as the authoritative repository root, loads the repository package configuration, and starts a localhost HTTP server. The server serves the Shell document and package assets, then dispatches package-owned API routes under `/api/<package-id>/...`.

## Package boundaries

- **Shell** owns the fixed browser bootstrap, primary navigation, package mounts, activation rollback, and shared layout.
- **Home** reads the configured repository landing source and renders it in the browser.
- **Docs** discovers Markdown beneath `HostContext.repositoryRoot`, exposes the tree/document routes, and re-reads documents whenever Docs is activated.
- **Chat** owns the server-side Pi ACP session, prompt submission, incremental activity events, transcript, and local recovery controls.

Packages contribute method-aware routes, assets, navigation metadata, and one browser module through the shared package contract. The host validates contributions transactionally, keys routes by `METHOD pathname`, and keeps optional package failures isolated. Package cleanup callbacks run in reverse registration order when the HTTP server closes; disposal is idempotent.

## Transport and safety boundaries

The shared HTTP layer bounds JSON request bodies and provides SSE framing. The server preserves `405` responses and lists registered methods in `Allow`. SSE handlers do not attempt a JSON response after headers are committed.

Filesystem access remains server-owned. Resonance remains localhost-only.
