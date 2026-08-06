# Pi Agent package

The Pi Agent package provides the default local Resonance workspace for one in-memory Pi ACP session. It is a developer-specific package and can be disabled in `.resonance/config.json`.

## Responsibilities

- Own the server-side ACP session manager and transcript.
- Lazily create at most one `pi-acp` process/session for the server.
- Submit prompts, enforce prompt concurrency, and bridge assistant/status updates through SSE.
- Reset the session and serve the Pi Agent browser workspace.

## Configuration

Configure Pi Agent as an entry in the repository manifest’s `packages` object:

```json
{
  "version": 1,
  "packages": {
    "pi-agent": { "module": "src/packages/pi-agent/index.ts" }
  }
}
```

- `module` is required at load time and must be a non-empty path relative to the Resonance application root.
- `enabled` is an optional common package flag; `false` omits Pi Agent from the host.
- Pi Agent has no package-specific configuration fields. Additional package inputs are ignored.

When registered, Pi Agent always creates its one in-memory session with `HostContext.repositoryRoot`; the browser cannot select a different filesystem root. The package also requires an installed `pi` executable and configured model credentials. Omit Pi Agent from `packages` or disable it when that local runtime is unavailable.

## ACP boundary

The server resolves the root-installed `pi-acp@0.0.33` entry through its dependency graph and starts it with `HostContext.repositoryRoot` as the child-process working directory. ACP `session/new.cwd` receives the same launch root. The browser never starts ACP or supplies a filesystem root.

## Routes and lifecycle

The browser submits JSON prompts to `POST /api/pi-agent/prompt`, listens to `GET /api/pi-agent/events`, reads `GET /api/pi-agent/state`, and resets with `POST /api/pi-agent/reset`. A new SSE subscriber receives a snapshot before live events. Incremental assistant and status events are normalized for the browser; tool-call lifecycle updates stay out of the transcript, and overlapping prompts are rejected with `409`.

The Pi Agent mount stays alive while hidden, but closes its `EventSource` when deactivated and reconnects on activation. This preserves the transcript without retaining hidden network subscribers. New Pi Agent sessions cancel and discard the current session and recreate it lazily on the next prompt.

## Runtime and safety

`pi-acp@0.0.33` requires an installed `pi` executable and configured model credentials. Pi Agent does not add MCP, authentication, tool controls, persistence, session history, remote access, or a second Markdown write policy; the installed Pi runtime and its configured permissions remain authoritative. After a fake or live Pi edit, re-enter Docs to discover and read changed Markdown from the same `repositoryRoot`.
