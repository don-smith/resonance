# Observability — Requirements

Role: owns the telemetry interface exposed to packages, the two-scope telemetry model (team and individual), local and remote exporter configuration, AI trace capture, and structured log infrastructure.

---

## Assumptions

- **RS.SYS.OBS-A01 Packages do not configure exporters.** Telemetry configuration is a runtime concern. Packages call a telemetry interface; where the data goes is invisible to them.

- **RS.SYS.OBS-A02 OpenTelemetry is the wire format.** The runtime uses the OpenTelemetry specification for spans, metrics, and logs. This allows any OTel-compatible backend as an exporter without changing the package interface.

- **RS.SYS.OBS-A03 Langfuse is the reference AI trace backend.** For AI interaction tracing, Langfuse is the default remote exporter in the individual scope. Other OTel-compatible backends are supported by configuration.

---

## Requirements

### Telemetry interface

- **RS.SYS.OBS-R01 The runtime provides a telemetry interface to packages.** Packages access telemetry through a Tauri command set: `telemetry.startSpan`, `telemetry.endSpan`, `telemetry.log`, `telemetry.recordMetric`. The interface is available to all packages without configuration. `refines: RS.SYS-R07`

- **RS.SYS.OBS-R02 Packages declare their telemetry namespace in the manifest.** Each telemetry call is automatically tagged with the emitting package's ID. Cross-package correlation uses a span context that the runtime propagates through the event bus. `refines: RS.SYS.PKG-R02`

### Two-scope model

- **RS.SYS.OBS-R03 Team scope covers shared system events.** The team telemetry scope captures: document sync events (peer connected, update applied, conflict resolved), workspace events (member added/removed, channel created), repo change events, and package lifecycle events (loaded, errored). Content is not captured in the team scope. Team telemetry stays local to each peer — it is not replicated. How team telemetry is shared and visualized is a real need but too early to lock down; revisit after initial implementation. `refines: RS.SYS-R08`

- **RS.SYS.OBS-R04 Individual scope covers personal AI interactions.** The individual telemetry scope captures: every agent invocation, model ID, token counts, tool calls made, tool results, latency, and (when enabled) the full prompt and response content. This scope is for personal evals and workflow tuning. `refines: RS.SYS-R08, RS.SYS-R10`

- **RS.SYS.OBS-R05 Each scope is configured independently.** Team scope configuration is a checked-in file (exporter URL, sampling rate, content capture flag). Individual scope configuration is a gitignored local file. A team member may configure their individual exporter without affecting team telemetry. `refines: RS.SYS-R09`

- **RS.SYS.OBS-R06 Individual scope content capture is opt-in.** AI prompt and response content is captured in the individual scope only when `RESONANCE_TELEMETRY_CAPTURE_CONTENT=true` is set in the individual config. The default is off. Team scope never captures content. `refines: RS.SYS-R10`

### Telemetry viewer

- **RS.SYS.OBS-R12 A built-in telemetry viewer package is provided.** The runtime ships a telemetry viewer package that reads from local telemetry files and displays traces, logs, and metrics. This reduces the setup barrier for new teams and serves as a natural vehicle for team-level visualizations — e.g., dashboards for qualitative and quantitative improvement metrics the team has agreed to track. The viewer also acts as the reference package for package authors implementing their own telemetry UIs. `refines: RS.CONTRIB-R05`

### Exporters

- **RS.SYS.OBS-R07 Local file exporter is the default for both scopes.** Structured JSON log files in the app data directory are always written, regardless of remote exporter configuration. Log rotation is runtime-managed. This provides a baseline for troubleshooting without any external service.

- **RS.SYS.OBS-R08 Remote exporters are optional and pluggable.** Any OTel-compatible endpoint may be configured as a remote exporter for either scope. The reference implementation ships with documented Langfuse configuration for the individual scope. `refines: RS.SYS-R09`

- **RS.SYS.OBS-R09 Exporter failures are silent.** A remote exporter that is unreachable or returns errors does not surface errors to the user or affect package behavior. Exporter errors are written to the local log only.

### Structured data

- **RS.SYS.OBS-R10 AI agent spans carry structured fields.** Every agent invocation span includes: `package.id`, `agent.model`, `agent.promptTokens`, `agent.completionTokens`, `agent.latencyMs`, `agent.toolCallCount`. The span carries a `session.id` for grouping related turns. `refines: RS.SYS-R10`

- **RS.SYS.OBS-R11 Secret-looking values are redacted.** Fields matching common secret patterns (API keys, tokens, passwords) are redacted before export in both scopes. Package authors do not need to sanitize their telemetry calls.

---

## Resolved Design Questions

- **RS.SYS.OBS-DQ01** Should the team scope telemetry be replicated to peers? **Deferred.** The two-scope model (team and individual) is the solid foundation — it enables two distinct learning journeys that should be transparent to each other. How team telemetry is shared and visualized is a real need but too early to lock down. The built-in telemetry viewer (see DQ02) is a natural vehicle for team-level dashboards when the team is ready to define what matters. Revisit after initial implementation.

- **RS.SYS.OBS-DQ02** Should there be a built-in telemetry viewer package? **Resolved: Yes.** A built-in telemetry viewer package (showing traces, logs, and metrics from local files) should be provided. This reduces the setup barrier and serves as a natural vehicle for team-level visualizations — e.g., dashboards for qualitative and quantitative improvement metrics that the team has agreed to track. The viewer can be the reference package (see `RS.CONTRIB-R05`).
