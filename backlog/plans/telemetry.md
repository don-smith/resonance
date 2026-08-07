# Telemetry

Owner: resonance

Status: scoped for implementation. The first milestone is instrumentation and Langfuse export for the Resonance process; a telemetry browsing agent is a later consumer of the same data.

## Decision

Telemetry is Resonance-owned host infrastructure, not a normal configured package. The host creates one process-level telemetry module and passes its package-safe interface through `HostContext`. This gives every team and member package one seam without making packages depend on Langfuse, adding a second manifest, or loading an omitted package implicitly.

The initial public package interface is intentionally small:

- structured `debug`, `info`, `warn`, and `error` events;
- child telemetry with fixed package/request fields;
- bounded spans for operations, including success, duration, and failure;
- an explicit flush/dispose lifecycle for process shutdown.

The interface must be safe for packages to call without awaiting network I/O. Exporters are adapters behind the interface. Exporter failures are reported locally and never make a package request fail.

## Ownership and placement

- `src/package-contract.ts` owns the shared `Telemetry` and field types because member packages need the same contract snapshot as other package capabilities.
- `src/telemetry.ts` owns the Resonance implementation, redaction, console output, exporter selection, and lifecycle.
- `src/host.ts` constructs or receives the process telemetry instance and adds it to the frozen `HostContext`.
- `src/server.ts` supplies request lifecycle fields and disposes telemetry with the host registry.
- Packages decide which domain events are useful, but do not construct exporters or read telemetry credentials.

The telemetry implementation must remain usable with no Langfuse credentials. The default console adapter is the immediate troubleshooting mechanism; Langfuse is an optional configured exporter. Configuration belongs to the Resonance process/environment rather than the viewed repository's package allowlist. Secrets must never be written to repository state, package state, logs, spans, prompts, model responses, or error payloads.

## First milestone

1. Add the shared interface and a console-backed implementation with level filtering, structured fields, error causes, and redaction of known secret keys.
2. Add an optional Langfuse adapter behind the same interface. It exports OpenTelemetry JSON traces through Langfuse's `/api/public/otel/v1/traces` endpoint, which is compatible with the local Langfuse v4 `events_only` deployment at `http://127.0.0.1:13000`; endpoint and credentials are repository-local process configuration, not package input.
3. Add request and startup telemetry in the host: package registration failures, method/path/status, duration, and uncaught handler errors. Query strings and request bodies are not recorded by default.
4. Instrument the Backlog package at the points needed to diagnose an apparently silent agent: prompt acceptance, selected-decision read, credential-required state, runtime creation, model stream start/end, mutation commits, reset/disposal, and the original failure cause. Preserve the user-facing generic errors while recording the internal cause through telemetry.
5. Flush on graceful server close with a bounded timeout. A telemetry outage must degrade to console diagnostics and must not prevent Resonance from serving.

The first Langfuse traces should describe Resonance requests and Backlog agent turns, with operation names, package/model/provider metadata, status, duration, and error information. Prompt, plan, tool, and model-output content is excluded by default and requires a separate explicit decision.

## Configuration direction

Use repository-local process configuration in the viewed repository's gitignored `.resonance/.env`, with `.resonance/.env.example` documenting the supported values. This keeps project-specific telemetry settings out of global shell configuration while keeping secrets out of the package manifest and committed repository state. The configuration needs to express:

- disabled, console-only, or Langfuse export;
- Langfuse base URL, defaulting to the local port when Langfuse export is explicitly enabled;
- public/secret credentials without exposing them to packages;
- minimum console level and a content-capture opt-in, defaulting to no content capture.

A later Resonance user-level configuration file may supplement repository-local environment variables. It must not become repository package configuration or revive implicit package loading.

## Telemetry workspace and agent

Do not create a `system` workspace scope or a telemetry package as part of the instrumentation milestone. A package is not synonymous with a workspace, and adding a third scope would expand the manifest and Shell model before it is needed.

A future read-only Telemetry workspace may be an explicitly configured Resonance-owned package. It would consume a narrow query adapter for the configured backend and could host a telemetry skill/agent. That agent must be separate from the instrumentation interface: packages emit telemetry through the host-owned interface, while the future workspace queries exported telemetry. Repository-specific telemetry can remain a separate member/team concern and must not be mixed into Resonance process telemetry by default.

## Verification

- Unit-test the telemetry interface with a deterministic in-memory adapter and exporter failures.
- Assert field redaction, level filtering, span completion/failure, bounded flush, and idempotent disposal.
- Assert host request instrumentation and Backlog lifecycle events without making network calls.
- Keep tests free of real credentials, prompts, plan contents, or Langfuse availability assumptions.
- Run `bun test src/packages/backlog/backlog.test.ts` and then `bun test` after implementation.

## Non-goals

- A general logging framework for arbitrary repository code.
- Automatic loading of a telemetry package or a second package manifest.
- Persistence, cross-process aggregation, remote access, or a hosted telemetry service.
- Capturing secrets or full repository/model content by default.
- Replacing the Backlog agent's user-visible SSE state with telemetry.
