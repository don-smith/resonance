## Architecture Overview

Resonance is a **local, manifest-driven workspace** for browsing and operating a repository. It follows a **package-based plugin architecture** where each package registers routes, assets, navigation, and a browser entry through a shared host contract.

---

## Package Contract

> `src/package-contract.ts`

This is the **foundational type system**. Every package conforms to the `PackageDefinition` interface:

| Concept               | Description                                                                                                                                           |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------- |
| `PackageDefinition`   | Metadata + a `register(context, input)` → `PackageRegistration` function                                                                              |
| `PackageRegistration` | Returns arrays of `routes`, `assets`, `navigation`, a `browser` entry, and optional `dispose`                                                         |
| `HostContext`         | Provides `repositoryRoot`, `appRoot`, `telemetry`, `state`, and `resolveRepositoryPath()` — the **only safe way** a package reads from the repository |
| `RouteContribution`   | Method + path + handler — paths must be **namespaced** under `/api/{packageId}/`                                                                      |
| `AssetContribution`   | Static files served under `/assets/{packageId}/` — paths validated to stay inside the package                                                         |
| `BrowserContribution` | JS entry + CSS stylesheet for the SPA mount                                                                                                           |

---

## Host Registry

> `src/host.ts` → `createHost()`

The host is the **composition root** that validates and assembles all packages:

1. **`createHost()`** iterates over loaded package definitions
2. For each package, it **validates**:
   - Route paths are namespaced (`/api/{id}/...`)
   - Asset paths are namespaced (`/assets/{id}/...`) — only Shell gets `/` and root-level app assets
   - Browser entry and stylesheet are registered assets
   - No duplicate route paths, asset paths, or navigation IDs
3. Packages can be **`team`** scoped (checked-in manifest) or **`member`** scoped (local `.resonance/member-config.json`)
4. Shell is **mandatory** — if it fails, the host throws

The `resolveRepositoryPath()` function prevents path traversal using `realpathSync` containment checks — this is the **package-safe repository read** pattern.

---

## Host Components

The Host container comprises three components:

| Component | Responsibility |
|-----------|---------------|
| **Package registry** | Loads the explicit package allowlist and validates contributions. |
| **Package transport** | Dispatches namespaced routes and bounded package-safe requests. |
| **Telemetry** | Structured logging and span export (console / Langfuse) with credential redaction. |

Telemetry is created by the registry and injected into all packages via `HostContext`. The `src/telemetry.ts` module (~16KB) provides structured logging, span export to both console and Langfuse, and automatic credential redaction to prevent secrets from appearing in logs.

---

## Package Loading

> `src/packages/index.ts` → `loadConfiguredPackages()`

1. Reads the **manifest allowlist** from `.resonance/config.json` (via `src/config.ts`)
2. Dynamically `import()`s each package's module — validates it exports a `PackageDefinition`
3. Loads **member packages** from a separate directory via `member-packages.json`
4. Binds configuration input to each package before returning definitions to the host

---

## The Five Packages

### 1. **Shell** (`src/packages/shell/`)

> _"Owns the browser frame, navigation, mounts, and bootstrap."_

- **Order 0** — always loaded first, always required
- Provides the HTML shell (`index.html`), the app bootstrap (`app.js`), and the SPA frame (`shell.js`, `styles.css`)
- Has privileged asset paths: `/`, `/assets/app.js`, `/assets/styles.css`
- No API routes — purely a UI container

### 2. **Home** (`src/packages/home/`)

> _"Renders the configured repository landing source."_

- **Order 10** — simple, single-route package
- Route: `GET /api/home` — reads a configured Markdown/HTML file (default: `README.md`) and renders it to HTML
- Uses `HostContext.resolveRepositoryPath()` for safe file access

### 3. **Docs** (`src/packages/docs/`)

> _"Discovers and renders repository Markdown documents."_

- **Order 20**
- Routes: `GET /api/docs/tree` (discovers all markdown), `GET /api/docs/document?path=...` (reads one)
- Uses `discoverMarkdownFiles()`, `buildMarkdownTree()`, `readMarkdown()` from `src/content.ts`
- Configurable extensions and ignored directories

### 4. **Architecture** (`src/packages/architecture/`)

> _"Projects LikeC4 architecture sources and runs bounded validation."_

- **Order 25** — the most complex package
- **Routes**: 12 total — model, views, graph projection, evidence reads, validation, and a **full agent loop**
  - `GET /api/architecture/model` — returns parsed LikeC4 model + views
  - `GET /api/architecture/views` — view definitions
  - `GET /api/architecture/graph?view=...&filter=...` — projected graph for a view
  - `GET /api/architecture/evidence?path=...` — bounded evidence file reads
  - `GET /api/architecture/validation` — runs architecture checkers
  - `GET /api/architecture/agent/state` + `/agent/events` (SSE) — agent snapshot + streaming
  - `POST /api/architecture/agent/prompt` — submits a prompt to the architecture agent
  - `POST /api/architecture/agent/credential` — stores API key securely (0600, non-tracked)
  - `POST /api/architecture/agent/reset` — resets the agent session
  - `POST /api/architecture/edit` — applies edits to model/views/rules/patterns/decisions (with confirmation for structural changes)
- **Internal modules**:
  - `architecture-store.ts` — reads/writes the LikeC4 model from the repository; handles stale-write detection (409 on revision mismatch)
  - `architecture-checkers.ts` — runs validation rules (6 rules defined in model: authoritative config, shell-required, package-ownership, namespacing, evidence containment, git revision)
  - `architecture-agent.ts` — agent session management
  - `architecture-deepagents.ts` — LLM runtime factory (OpenAI/OpenRouter)
  - `architecture-likec4.tsx` — LikeC4 React integration
  - `architecture-source.js` → 2.5MB bundle — the LikeC4 browser runtime
- Requires LLM credentials stored at `.resonance/architecture-agent.env` (mode 0600, must not be git-tracked)

### 5. **Backlog** (`src/packages/backlog/`)

> _"Owns the canonical backlog projection and constrained backlog agent."_

- **Order 30**
- **Routes**: 7 total
  - `GET /api/backlog/items` — lists all backlog decisions
  - `GET /api/backlog/plan?path=...` — reads a single decision and renders it
  - `GET /api/backlog/agent/state` + `/agent/events` (SSE) — agent streaming
  - `POST /api/backlog/agent/prompt` — submits a prompt (requires `selectedPath`)
  - `POST /api/backlog/agent/credential` — stores API key (same 0600 pattern)
  - `POST /api/backlog/agent/confirm-deletion` — confirms agent-requested deletions
  - `POST /api/backlog/agent/reset` — resets the session
- **Internal modules**:
  - `backlog-store.ts` — discovers and parses backlog decision files
  - `agent-session.ts` — agent session with subscription-based event streaming
  - `deepagents.ts` — LLM runtime

---

## Key Infrastructure Patterns

### Package-Safe Repository Read

```typescript
// src/host.ts — HostContext.resolveRepositoryPath()
// Uses realpathSync containment to prevent path traversal
const relative = path.relative(root, absolute);
if (relative === ".." || relative.startsWith(`..${path.sep}`)) return null;
```

All file reads go through `resolveRepositoryPath()` — packages never access the filesystem directly.

### Namespaced Routes & Assets

Every package route is validated to stay under its namespace:

- Routes: `/api/{packageId}/...`
- Assets: `/assets/{packageId}/...`

### Dual-Scope Packages

- **Team** packages: configured in `.resonance/config.json`, committed to the repository
- **Member** packages: configured in `.resonance/member-config.json`, sourced from a separate directory via `member-packages.json`

### Agent-Capable Packages

Packages with LLM agents (Architecture, Backlog) follow a consistent contract pattern:
- SSE streaming via `GET /api/{package}/agent/events`
- Prompt submission via `POST /api/{package}/agent/prompt`
- Credential management via `POST /api/{package}/agent/credential`
- Session reset via `POST /api/{package}/agent/reset`

### Package State

Packages can persist JSON state via `PackageState` (stored in `.resonance/state/{packageId}/state.json`, max 64KB, validated at startup).

### Credential Security

Agent-capable packages (Architecture, Backlog) follow the same pattern:

1. Credentials stored in `.resonance/{package}-agent.env`
2. Must be mode `0600`, regular files, not symlinks
3. Must NOT be git-tracked
4. Atomic writes via temp file + rename

### SSE Agent Streaming

Both Architecture and Backlog use Server-Sent Events for real-time agent communication:

- `GET /api/{package}/agent/events` opens an SSE stream
- `POST /api/{package}/agent/prompt` submits prompts, returns immediately with 202
- Events flow asynchronously through the SSE stream

### HTTP Server (`src/server.ts` → `createApp()`)

The server composes everything:

1. Loads config + member config
2. Creates the host with all packages
3. Dispatches requests: manifest → routes → assets → 404
4. Assets served from package roots via `createReadStream`

---

## Summary of the Dependency Flow

```
.resonance/config.json  ──►  config.ts  ──►  packages/index.ts  ──►  host.ts  ──►  server.ts
                                                   │                      │
                                          dynamic import()         createHost()
                                          of each package          validates & composes
                                                   │                      │
                              ┌────────────────────┼──────────────────────┤
                              │                    │                      │
                          shell/              architecture/          backlog/
                          home/               (12 routes,            (7 routes,
                          docs/                agent, store,          agent, store,
                          (1-2 routes,         checkers,              deepagents)
                           content)            deepagents)
```

---

## Architecture Validation Rules

The Architecture package runs 6 deterministic rules against the LikeC4 model:

1. **Authoritative package configuration** — repository uses an explicit version 1 package allowlist
2. **Shell is required** — the Shell package must be present in the manifest
3. **Package ownership** — all modeled packages have explicit manifest ownership
4. **Namespaced package contributions** — routes and assets use package namespaces
5. **Repository evidence contained** — all modeled evidence paths remain within the viewed repository
6. **Reviews identify a Git revision** — a Git revision source is available for architecture reports
