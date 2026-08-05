# Chat package

Chat is the default local Resonance workspace for one in-memory Pi ACP session. It can be disabled in `.resonance/config.json`.

## Responsibilities

- Own the server-side ACP session manager and transcript.
- Lazily create at most one `pi-acp` process/session for the server.
- Submit prompts, enforce prompt concurrency, and bridge assistant/status updates through SSE.
- Reset the session and serve the Chat browser workspace.

## ACP boundary

The server resolves the root-installed `pi-acp@0.0.33` entry through its dependency graph and starts it with `HostContext.repositoryRoot` as the child-process working directory. ACP `session/new.cwd` receives the same launch root. The browser never starts ACP or supplies a filesystem root.

## Routes and lifecycle

The browser submits JSON prompts to `POST /api/chat/prompt`, listens to `GET /api/chat/events`, reads `GET /api/chat/state`, and resets with `POST /api/chat/reset`. A new SSE subscriber receives a snapshot before live events. Incremental assistant and status events are normalized for the browser; tool-call lifecycle updates stay out of the transcript, and overlapping prompts are rejected with `409`.

The Chat mount stays alive while hidden, but closes its `EventSource` when deactivated and reconnects on activation. This preserves the transcript without retaining hidden network subscribers. New Chat cancels and discards the current session and recreates it lazily on the next prompt.

## Runtime and safety

`pi-acp@0.0.33` requires an installed `pi` executable and configured model credentials. Chat does not add MCP, authentication, tool controls, persistence, session history, remote access, or a second Markdown write policy; the installed Pi runtime and its configured permissions remain authoritative. After a fake or live Pi edit, re-enter Docs to discover and read changed Markdown from the same `repositoryRoot`.
