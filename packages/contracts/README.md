# Resonance package contract

`manifest.v1.json` is the public package-manifest interface. It is validated by
both the TypeScript authoring adapter and the Rust runtime registry against the
same fixtures. `manifestVersion: 1` is deliberately explicit: changing this
schema, roles, standard events, or capability vocabulary requires an RFC.

## Bundled team packages only

Phase 1 accepts only reviewed `bundled-team` manifests. Member-local and
repository loaders are deferred until separate least-privilege webviews can
enforce their access. A package ID is lowercase `namespace.name`; the reference
namespace is `resonance.*`.

Roles are `viewer`, `contributor`, and `developer`. Standard events are
`repo:changed`, `doc:updated`, `doc:opened`, `message:received`, `peer:joined`,
`peer:left`, `workspace:member-added`, and `workspace:member-removed`. A package
may declare its own lowercase `namespace:event` or `agent-context:event` name.
The runtime routes declared events without interpreting their payloads.

Capabilities are semantic declarations, not Tauri command names:
`documents:read`, `documents:write`, `workspace:read`, `repository:read`, and
`telemetry:write`. Agent permissions are likewise semantic: `read`,
`suggest-edits`, `apply-edits`, `create-documents`, and `post-messages`. An
agent declaration contains `systemPrompt`, `permissions`, and
`contextProviders`; it never contains panel HTML because the runtime owns the
shared panel.

## Generate a manifest

```sh
pnpm --filter @resonance/contracts generate -- \
  --id resonance.my-package --output packages/my-package
pnpm --filter @resonance/contracts test
```

The generator writes `manifest.json` from the versioned template. See
[`../reference-package/manifest.json`](../reference-package/manifest.json) for a
worked validated package.
