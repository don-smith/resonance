# System — Requirements

Role: owns the technical contracts that all subsystems must satisfy, and coordinates the boundaries between identity, transport, documents, conversations, packages, repos, observability, and the agent panel.

---

## Assumptions

- **RS.SYS-A01 Tokio is the application async runtime.** The Tauri backend and ordinary Rust subsystems compose on Tokio. The production Commonware conversation adapter is the bounded exception: it owns Commonware's blocking Tokio runner on one named OS thread and communicates with the application through bounded queues. SQLite, authority replay, and conversation cryptography remain outside that thread.

- **RS.SYS-A02 The webview is the UI layer.** All user interface is implemented in the system webview. Rust provides capability (commands, events) but never renders UI directly.

- **RS.SYS-A03 SQLite is the local persistence store.** Structured data, including conversation archives, workspace member lists, workspace-file history, registered repositories, and telemetry, is persisted in a local SQLite database via `rusqlite`. Immutable workspace-file blobs are stored as private files.

---

## Requirements

### Subsystem boundaries

- **RS.SYS-R01 Subsystems communicate through defined interfaces.** Identity owns installation signing and recipient-key custody; workspace membership owns canonical authority; Iroh transport owns workspace delivery; conversation runtime owns canonical records, cryptography, storage, and its Commonware mesh. Subsystems do not reach into each other's internal state or expose infrastructure handles to packages. `refines: RS-R10`

- **RS.SYS-R02 The event bus is the cross-package boundary.** Rust-to-webview communication uses Tauri events for notifications and Tauri commands for request/response. Package-to-package communication uses events only. `refines: RS-R08, RS-R10`

- **RS.SYS-R03 The runtime owns no user-visible content.** The app shell, navigation, workspace member list display, and status indicators are runtime responsibilities. All content views (documents, conversations, repo views) belong to packages. `refines: RS-R08`

### Agent panel

- **RS.SYS-R04 The agent panel is a first-class runtime surface.** Every package may declare an agent configuration (system prompt, permissions, context providers). The runtime renders the agent panel and routes agent interactions. Packages do not implement their own agent UI; they configure the shared runtime panel. `refines: RS-R08`

- **RS.SYS-R05 Agent context is package-owned.** Each package's agent receives context injected by the package: the currently selected document, highlighted text, selected backlog item, active architectural node. The runtime provides the injection mechanism; the package declares what context it provides and under what conditions.

- **RS.SYS-R06 Agent permissions are package-scoped.** A package declares the operations its agent may perform: read-only, may suggest edits, may apply edits, may create documents, may post messages. The runtime enforces these permissions; the agent runtime does not have elevated access beyond what the package declares.

- **RS.SYS-R13 The agent panel is a right-side panel with a consistent, collapsible UI.** The panel occupies the right-hand side of the web interface, leaving the left-hand navigation to the runtime shell and the center to the active package's content view. The panel is collapsible when not in use. Every package's agent panel shares an identical UI shell: context window, send button, new-chat button, ready/working indicator, and scope selector. Packages configure only the package-level parameters (system prompt, permissions, context providers, scope); they do not customize the panel's layout or controls. `refines: RS.SYS-R04`

### Observability

- **RS.SYS-R07 The runtime provides a telemetry interface to packages.** Packages emit structured telemetry (logs, spans, traces) through a runtime-provided interface. Packages do not configure exporters; the runtime manages export. `refines: RS-R08`

- **RS.SYS-R08 Two telemetry scopes exist: team and individual.** Team telemetry covers shared processes (document sync events, workspace activity, package-initiated operations). Individual telemetry covers personal agent interactions, AI traces, and local workflow evals. Each scope is configured independently with its own exporter target. `refines: RS.SYS-R07`

- **RS.SYS-R09 Telemetry exporters are local-first by default.** The default exporter writes to local structured log files. Remote exporters (OpenTelemetry-compatible endpoints, Langfuse for AI traces) are configured per-scope in gitignored config files. No telemetry leaves the machine without explicit exporter configuration. `refines: RS.SYS-R07`

- **RS.SYS-R10 AI interactions are traced.** Every agent invocation (request, model response, tool calls, token counts) is captured as a trace in the individual telemetry scope. The team telemetry scope captures aggregate agent usage (invocation counts, latency, errors) without capturing content, unless content capture is explicitly enabled.

### Safety

- **RS.SYS-R11 Package filesystem access is bounded.** Packages access the filesystem through explicit Tauri commands, not direct filesystem APIs. The runtime enforces path containment. A package may not traverse above its declared root.

- **RS.SYS-R12 Credentials are never passed to packages.** The runtime does not expose installation signing secrets, conversation recipient private keys, epoch keys, workspace tokens, or raw signed records to the webview. Signing, wrapping, sealing, and authenticated opening are performed in Rust; packages receive bounded semantic views only.

### Persistence

- **RS.SYS-R14 SQLite databases are per-workspace.** Each workspace has its own SQLite database, providing clean isolation and easy export. The runtime manages database lifecycle per workspace. When only one workspace exists, the workspace concept may be hidden from the UI, presenting the default workspace transparently. `refines: RS.SYS-A03`
