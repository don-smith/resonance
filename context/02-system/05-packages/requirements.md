# Packages — Requirements

Role: owns the package model, the package manifest contract, the event bus wiring, the agent panel contract, and the team/member package distinction. Defines what a package is, what it can declare, and what the runtime provides.

---

## Assumptions

- **RS.SYS.PKG-A01 Package creation must feel fluid and well-supported, regardless of language.** The primary goal is a great developer experience: clear expectations, good generators, and AI-assisted scaffolding. If Rust is needed, it should be generated with transparent explanations of what it does and why. If TypeScript or web technologies suffice, that path should be equally well-supported. The team should never feel lost or uncertain about the architecture of their package.

- **RS.SYS.PKG-A02 Packages are trusted by the team.** Team packages are reviewed and committed by someone with commit access to the team's fork. The runtime does not sandbox team packages against each other; it relies on team review.

---

## Requirements

### Package model

- **RS.SYS.PKG-R01 A package is a webview module with an optional Rust side.** The package's UI is an HTML entry point loaded into a Tauri webview tab. The optional Rust side registers Tauri commands. The runtime owns the tab lifecycle.

- **RS.SYS.PKG-R02 Packages are declared in a manifest.** Each package has a `manifest.json` declaring: `id` (globally unique within the installation), `name`, `description`, `nav` (navigation entry metadata), `events.emits[]`, `events.consumes[]`, `agent` (optional), `minRole` (`viewer` | `contributor` | `developer`). The runtime validates the manifest at load time.

- **RS.SYS.PKG-R03 Package IDs are namespaced.** Team packages use the team's namespace (e.g., `acme.backlog`). Reference packages use the `resonance.*` namespace. Member packages use `member.<id>`. Namespace collisions are rejected at load time.

- **RS.SYS.PKG-R16 Package creation is scaffolded, not authored from scratch.** The runtime provides generators and AI agents that produce a complete, well-documented package skeleton. The generated code includes inline explanations of each file's purpose, the architecture decisions made, and any Rust or TypeScript patterns used. The team member should be able to understand the full package architecture without external documentation. `refines: RS.SYS.PKG-A01`

### Event bus

- **RS.SYS.PKG-R04 Events are the cross-package communication channel.** Packages call `emit(eventName, payload)` and `listen(eventName, handler)`. The runtime routes events. Packages do not hold references to other packages. `refines: RS-R10`

- **RS.SYS.PKG-R05 A package may only emit events it declares.** In development mode, the runtime rejects undeclared emits and logs a warning. In production mode, undeclared emits are silently dropped to avoid crashes. `refines: RS.SYS.PKG-R02`

- **RS.SYS.PKG-R06 Standard events are defined by the runtime.** The runtime defines a vocabulary of standard events that packages use for common interactions. Packages may define additional domain-specific events, which must be declared in the manifest. Standard events include: `repo:changed`, `doc:updated`, `doc:opened`, `message:received`, `peer:joined`, `peer:left`, `workspace:member-added`, `workspace:member-removed`.

### Agent panel

- **RS.SYS.PKG-R07 Any package may declare an agent and an agent panel.** The agent declaration in `manifest.json` specifies: `systemPrompt` (path to a Markdown file), `permissions` (list of allowed operations), `contextProviders` (list of context keys the package will inject), and `agentPanel` (path to the HTML entry point for the agent panel UI). Declaring an agent signals that the package wants an agent panel. The runtime provides composable UI components and scaffolding to help the package render the panel consistently. `refines: RS.SYS-R04`

- **RS.SYS.PKG-R08 Context injection is event-driven.** When the agent panel is open, the package emits context events (e.g., `agent-context:document-selected`, `agent-context:text-highlighted`, `agent-context:node-selected`) as the user interacts with its UI. The runtime subscribes to these events and passes the current context to the agent on each invocation. The package may use runtime-provided context injection helpers to simplify this flow. `refines: RS.SYS-R05`

- **RS.SYS.PKG-R09 Agent permissions are declarative.** The `permissions` array in the agent declaration lists the Tauri commands the agent may invoke. The runtime enforces this list; the agent runtime cannot call commands outside it. `refines: RS.SYS-R06`

- **RS.SYS.PKG-R10 The package owns the agent panel UI.** The package declares that it wants an agent and an agent panel in its manifest. The package is responsible for rendering the agent panel UI, which is expected to appear as a right-hand panel. The runtime provides rendering helpers, UI components, and scaffolding to ensure a consistent look and feel across packages. The runtime also provides the chat interface, model selector, and history display as composable components that the package can use. The package configures the agent's behavior through the manifest and context events. `refines: RS.SYS-R04`

### Team and member packages

- **RS.SYS.PKG-R11 Team packages are compiled into the binary.** Team packages are part of the forked repository. They ship with each app update.

- **RS.SYS.PKG-R12 Member packages are loaded at runtime from a local path.** Member packages are loaded from a path in local member configuration. They are not compiled into the binary and do not affect other team members. `refines: RS-R11`

- **RS.SYS.PKG-R13 Team packages win contribution conflicts.** If a team package and a member package declare the same navigation slot or event handler, the team package wins. The runtime logs the conflict but does not crash.

### Repo packages

- **RS.SYS.PKG-R14 Repo packages are loaded from the repository manifest.** When a repository is registered and its `.resonance/config.json` exists, the runtime loads the packages declared in that manifest as repo packages for that repository. Repo packages are a subset of team packages: they are checked in, team-agreed, and may be replaced by individual member configuration. `refines: RS-R16`

- **RS.SYS.PKG-R15 Repo packages receive a repository context.** The runtime injects the repository root path and the git watcher event stream into repo packages. Repo packages may read from the repository path via bounded Tauri commands.

---

## Open Design Questions

- **RS.SYS.PKG-DQ01** How does a single package invoke multiple agents? The runtime supports one agent per package — each package gets its own agent instance with independent state. As the user flips between packages in the UI, they see that package's agent and its current state. Multiple agents can run concurrently across packages. The open question is how a single package invokes *multiple* agents (potentially from different models), since the agent panel is a right-hand singleton by UI convention. A future notification mechanism (e.g., indicating which agents are working, completed, or waiting on user input) is acknowledged but deferred.


