# Member packages

Owner: team member

## Decision

Resonance supports two package scopes:

- **Team packages** are shared, repository-defined packages. They are selected in the checked-in `.resonance/config.json` and are visible to every developer using the repository.
- **Member packages** are developer-specific packages. Each developer maintains one external member-package repository, and selects packages for each viewed repository through ignored local configuration.

Both scopes use the same package contract, but team packages are authoritative when the scopes conflict. A member package must not override a team package's ID, route, asset, or navigation contribution.

Packages and workspaces remain distinct concepts. A member package may provide a Personal workspace, but the package scope does not require it to provide a workspace.

## Member package repository

Each developer has one member-package repository containing an explicit manifest with module paths relative to that repository:

```json
{
  "version": 1,
  "packages": {
    "pi-agent": {
      "module": "src/packages/pi-agent/index.ts"
    }
  }
}
```

Member packages are loaded from the live checkout. No package version or repository revision is pinned in v1, so changes to the checkout are available to every viewed repository that selects the package.

The member package repository is external to the viewed repository. Resonance must resolve member module, asset, and browser-entry paths relative to the member repository while preserving package-contract validation and containment rules.

## Repository-local member configuration

Member installation writes an ignored `.resonance/member-config.json` in the viewed repository. It records the external source path, selected package IDs, and package-specific inputs; it does not duplicate module paths:

```json
{
  "version": 1,
  "source": "/path/to/member-packages",
  "packages": {
    "pi-agent": {}
  }
}
```

The checked-in team `.resonance/config.json` remains authoritative for team packages and must not contain member-package selections. There is no user-global member-package catalog in v1.

`resonate member install <member-repository>` registers the source path for the current viewed repository, reads the member manifest, interactively selects packages, writes the ignored member config, and adds narrow `.gitignore` entries for the member config and member state. It preserves existing `.gitignore` content and does not ignore the checked-in team config.

## Loading and navigation

Resonance loads team packages first and member packages second. Team packages always win conflicts. A conflicting member package is skipped and produces a visible diagnostic rather than preventing team startup.

Member configuration, source, manifest, module, registration, and state failures are nonfatal: Resonance continues with team packages, reports the failure to the console, and exposes an identifiable disabled/failed member entry in the UI when possible.

Shell presents package navigation in two sections:

1. **Team**, first and unaffected by member ordering.
2. **Personal**, shown when member packages contribute navigation.

Member package changes become effective from the live checkout on the next load/restart. Promotion or demotion between package scopes is manual and is not part of this decision.

## Package state

The host provides a common, package-scoped state capability for both package scopes. State is limited in v1 to one bounded JSON document per package; packages do not receive an arbitrary filesystem API. Team and member state use separate physical roots, with member state under `.resonance/member-state/<package-id>/`.

Missing state returns no value so a package can initialize it. Malformed or unreadable state is not overwritten automatically; only the affected optional package is disabled and the failure is surfaced. State write failures produce an actionable package diagnostic. Existing team-specific credential storage is not migrated by this decision.

Pi Agent initializes its member state on first use. It derives a stable repository label from the Git `origin` remote, then the first parseable remote, then the repository directory basename. It stores the label without re-checking or silently changing it later. Telemetry is explicitly deferred.

## Migration and completion

The existing Pi Agent package is the first member package to migrate. Its package contract and local Pi runtime requirements remain otherwise unchanged; it is removed from team-package installation and installed through the member-package flow.

Out of scope:

- Automatic promotion or demotion between team and member packages.
- A user-global member-package catalog.
- Package revision pinning or reproducible member-package snapshots.
- Telemetry.
- Arbitrary member state files, state migrations, or credential migration.
- Directory-scanning package discovery.

The decision is complete when a developer can install team Resonance, run `resonate member install` against one external member repository, select Pi Agent, and start Resonance with Team and Personal navigation. The live member checkout is used, Pi Agent can initialize its package state, and missing, invalid, or conflicting member packages do not prevent team packages from starting.
