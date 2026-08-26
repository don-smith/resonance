# Repos — Requirements

Role: owns repository registration, git watching, repository event emission, multi-repo switching, and the boundary between repository content and workspace content.

---

## Requirements

### Registration

- **RS.SYS.REPO-R01 Repositories are registered by local path.** A team member adds a repository by providing its local filesystem path. The path must point to a directory containing a `.git/` subdirectory. Registration is stored in local app configuration (not in the workspace; repository registration is per-device).

- **RS.SYS.REPO-R02 Multiple repositories may be registered.** There is no limit on the number of registered repositories. The app shell provides a repository switcher in navigation. `refines: RS-R16`

- **RS.SYS.REPO-R03 A registered repository does not require a package manifest.** If `.resonance/config.json` is absent, the runtime loads no repo packages for that repository but still provides the git watcher event stream and allows future package installation.

### Git watching

- **RS.SYS.REPO-R04 The runtime watches the registered repository for changes.** Using filesystem watching (`notify` crate), the runtime monitors `.git/HEAD`, `.git/index`, and the working tree for changes. Debounced changes emit a `repo:changed` event on the event bus. `refines: RS.SYS.PKG-R06`

- **RS.SYS.REPO-R05 The repo:changed event carries a structured payload.** The event payload includes: `repoId`, `repositoryPath`, `head` (current commit hash and branch name), `status` (dirty/clean), `changedPaths` (list of modified paths, capped at 50 for performance). Packages use this event to refresh their views.

- **RS.SYS.REPO-R06 Repository reads are bounded.** Repo packages read repository files through explicit Tauri commands. The runtime enforces containment: paths are resolved relative to the registered repository root and may not traverse above it. Symlinks that escape the repository root are rejected.

### Boundary with workspace content

- **RS.SYS.REPO-R07 Repository files are never written by the sync layer.** The transport and document subsystems do not write to registered repository paths. The only mechanism for writing to a repository is through an explicit, user-initiated Tauri command (e.g., "save document edit for commit"). `refines: RS-R05`

- **RS.SYS.REPO-R08 Repository content requires a local clone.** Resonance does not replicate repository files, snapshots, or repository-derived read-only views to devices without a registered local clone. Repository packages operate only against local clones through bounded runtime operations. Members without a clone use workspace files and conversations for shared team context. `refines: RS-R05`
