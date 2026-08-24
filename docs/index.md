# Resonance user documentation

Resonance gives a team one shared file tree while each member keeps a separate local folder. Files remain ordinary files. Markdown can be edited in Resonance or with local tools, binary files retain verified bytes, and unsafe concurrent changes remain visible until someone resolves them.

For the designed browser version of these docs, open [`html/index.html`](html/index.html).

## Start here

### Work with workspace files

Use [Workspace files](workspace-files.md) to:

- choose or recover a private workspace folder;
- create and edit Markdown in Resonance;
- add or change files with local tools;
- understand synchronization and conflict handling;
- check current limits before moving existing work into a workspace.

## The basic model

Every member chooses a new or empty local folder outside Git management. Resonance materializes the shared workspace tree into that folder, beginning with `plans`. Members can use different paths and folder names on their own computers while sharing the same logical tree and file bytes.

Resonance keeps its keys, database, operation history, and blob store outside the shared folder. Files in the folder are workspace content, not Resonance control data.

## Current user-facing capabilities

- Browse nested workspace folders and files.
- Create and edit rendered Markdown files.
- Add or edit ordinary files with local applications.
- Preview supported images and inspect folder contents.
- Synchronize signed file history and verified binary content with authorized peers.
- Merge disjoint Markdown edits automatically.
- Review and resolve Markdown, binary, deletion, creation, and move conflicts without silently discarding a competing change.
- Repair, replace, or unbind a private workspace folder.

## Technical references

These references explain implementation and project decisions rather than ordinary product use:

- [Local workspace data](local-data.md) describes private storage, root materialization, recovery, and ignore rules.
- [Filesystem-first workspace authority](../context/.decisions/0009-filesystem-first-workspace-authority.md) records the accepted architecture decision.
- [Document requirements](../context/02-system/03-documents/requirements.md) define the authoritative behavior and boundaries.
- [Fork and release guide](fork-guide.md) is for teams that build and distribute their own Resonance fork.
- [Package authoring](package-authoring.md) is for developers creating reviewed bundled packages.
