# Workspace files

Resonance gives every member an ordinary local folder for one shared file tree. You can edit Markdown inside Resonance, work with files in local applications, disconnect, and synchronize later. Resonance merges only changes it can prove are safe. Other races remain visible for a person to resolve.

For the designed browser version of this guide, open [`html/workspace-files.html`](html/workspace-files.html).

## Before you begin

You need an active Resonance workspace. Choose a folder that is:

- new or empty;
- outside a Git repository;
- available and writable;
- not a symlink.

Do not choose an existing project folder. Importing a non-empty folder is not supported yet.

## Choose your workspace folder

1. Open the workspace and find **Files**.
2. Select **Choose folder**.
3. Pick a new or empty folder outside Git management.
4. Confirm **Use folder**.
5. Wait for the file tree to appear. A new workspace begins with `plans`.

Your folder path is private to your computer. Another member can choose a different folder and still receive the same workspace tree and file bytes.

Resonance does not place its database, keys, tokens, blob store, or watcher metadata in this folder.

## Work with files

### Create and edit Markdown in Resonance

1. Under **Files**, choose a destination from **Folder**.
2. Enter a file name ending in `.md`.
3. Select **New Markdown file**.
4. Write in the rendered editor.
5. Select **Save revision**.

The rendered editor accepts Markdown files up to 1 MiB. Raw Markdown mode and live collaborative cursors are not available yet.

If a peer or local application changes the same file while it is open, Resonance reports that a newer workspace revision is available and keeps your editor draft unchanged. Presence updates, unrelated file-tree changes, deletion, and conflict updates also leave the draft open.

To inspect a newer revision without losing your work:

1. Select **Review latest**. Resonance fetches and renders that revision while retaining your draft.
2. Select **Return to draft** to continue editing the unchanged draft, or select **Load latest and replace draft** to adopt the reviewed content and revision as your new editing base.

Loading is the only action that replaces both the draft and its base revision. Saving the older base still fails instead of overwriting current authority. The rejection leaves the full draft open so you can review the latest revision and decide what to carry forward.

### Use local applications

You can create folders, edit Markdown, and add ordinary files directly inside the bound folder. Resonance waits for a file to become stable before it records the change. You do not need to upload it separately.

Binary content is stored and transferred by verified content hash. Supported images up to 10 MiB can be previewed in Resonance. Open other binary formats with a local application.

Avoid editing files whose names contain `.resonance-conflict-`. Resonance creates those artifacts to preserve competing work, and it does not ingest them as new user changes.

### Work offline

Keep editing the bound folder while peers are unavailable. Resonance records stable local changes and exchanges missing history and file bytes after authorized peers reconnect.

Two offline Markdown edits merge automatically only when they change disjoint lines from the same base. A merge that could discard intent becomes a visible conflict instead.

## Understand conflicts

Resonance does not pick a winner when two changes cannot be combined safely. It preserves the competing versions or tree choices and lists the unresolved record under **Conflicts**.

| Conflict                   | What happened                                         | What you can decide                                                 |
| -------------------------- | ----------------------------------------------------- | ------------------------------------------------------------------- |
| Overlapping Markdown edits | Members changed overlapping lines from the same base. | Review available revisions and choose the content to keep.          |
| Competing binary revisions | Members changed the same binary file.                 | Choose the revision to keep.                                        |
| Delete and edit conflict   | One member deleted a file while another edited it.    | Keep the edited file or keep the deletion.                          |
| Competing file creation    | Members created different entries at the same path.   | Preview the candidates and choose the file or folder to keep there. |
| Competing file moves       | Members moved the same entry to different locations.  | Preview the choices and select the location to keep.                |

To resolve a conflict:

1. Read the conflict label under **Conflicts**.
2. Use **Review** or **Preview** where available.
3. Select the revision, entry, deletion, or location to keep.
4. Wait for the resolution to synchronize to other connected members.

Resolution closes the visible conflict. Resonance retains signed history rather than erasing the discarded branch.

## Repair or change the folder

The status beside **Files** reports whether the bound folder is healthy.

### Repair

Use **Repair** after restoring the same folder or making it writable again. Resonance repairs interrupted projection work and materializes the current shared tree.

### Replace

Use **Replace** when you want to bind a different folder. The replacement must also be new or empty and outside Git management. Resonance materializes the shared tree into the new location.

### Unbind

Use **Unbind** to disconnect the current folder. This clears the private binding. It does not delete shared workspace history or remove the existing files from disk. You can bind another eligible folder later.

File-history synchronization can continue while a bound folder is unavailable. Local materialization resumes after the folder becomes healthy or you replace it.

## Names and content to avoid

Resonance rejects or ignores paths that cannot synchronize safely across members. In the current release:

- keep workspace roots outside Git repositories;
- do not add `.git` directories;
- do not add symlinks or special files;
- avoid names that differ only by letter case;
- use Unicode names in NFC form;
- do not use operating-system device names or generated conflict names;
- do not expect permission bits, executable bits, or case-only renames to synchronize.

SVG files are not shown as inline previews yet. Open them with a local application.

## Current boundaries

The first filesystem workspace release does not yet support:

- importing a non-empty root;
- raw Markdown mode or live carets;
- symlinks, special files, permissions, or executable bits;
- case-only renames;
- history compaction, garbage collection, or manual history pruning;
- a scaling policy for multi-gigabyte files or very large trees;
- inline SVG preview.

If a folder is rejected, choose a new empty folder outside Git management. If a Markdown save reports that the file changed, keep the open draft, review the latest revision, and decide whether to return or replace it. If a conflict appears, resolve it in Resonance rather than deleting generated conflict artifacts by hand.

## Technical detail

The `resonance.workspace-files` bundled TypeScript package owns this interface, including the tree, editor sessions, previews, root actions, conflict review, and cleanup. It uses only the documented `workspace-files:v1` SDK capability. Rust still owns file authority, private root custody, blobs, persistence, signing, conflict computation, and peer recovery. Package-visible snapshots and errors contain no local path or transport state.

See [Package authoring](package-authoring.md) for the package boundary and [Local workspace data](local-data.md) for private storage and recovery. The architecture is recorded in [Decision 0009](../context/.decisions/0009-filesystem-first-workspace-authority.md) and [Decision 0010](../context/.decisions/0010-source-bundled-content-host.md). Normative file behavior lives in the [document requirements](../context/02-system/03-documents/requirements.md).
