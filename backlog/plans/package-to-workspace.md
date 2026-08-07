# Package to workspace

Owner: team

## Decision

Keep **package** and **workspace** as distinct concepts.

A Resonance package is the general extensibility and implementation unit. A workspace is a user-visible surface mounted and navigated by Shell. A package may provide a workspace, but packages may also provide infrastructure or future non-UI capabilities. Shell is a required infrastructure package, not a workspace.

Home, Docs, and Backlog are packages that currently provide user-visible workspaces. External member packages may also provide workspaces. This relationship remains descriptive for now; workspace metadata and separate workspace identifiers remain deferred.

## Scope

- Use **package** for source modules, manifest entries, package IDs, routes, assets, contracts, and the package authoring CLI.
- Use **workspace** for user-facing mounted and navigable surfaces.
- Update architecture, README, package authoring guidance, and package documentation to explain the distinction.
- Do not rename package directories, imports, configuration keys, routes, CLI commands, or runtime contracts.
- Do not introduce team/member configuration in this decision.

## Completion

The distinction is documented in the architecture, root README, package authoring skill, and relevant package READMEs. No runtime behavior changes are required.
