export const workspaceFilesErrorMessages = Object.freeze({
  "unavailable-capability": "Workspace files are unavailable.",
  "invalid-request": "The workspace-files request is invalid.",
  "missing-revision": "That file revision is unavailable.",
  "stale-revision": "The file changed before this edit could be saved.",
  "invalid-markdown-name": "Markdown file names must end in .md.",
  "size-limit": "The workspace-files size limit was exceeded.",
  "unusable-root": "Choose a usable workspace folder.",
  "changed-conflict-choice": "That conflict choice is no longer available.",
  internal: "Workspace files could not complete the request.",
});
export type WorkspaceFilesErrorCode = keyof typeof workspaceFilesErrorMessages;
export type WorkspaceFilesError = Readonly<
  {
    [Code in WorkspaceFilesErrorCode]: {
      code: Code;
      message: (typeof workspaceFilesErrorMessages)[Code];
    };
  }[WorkspaceFilesErrorCode]
>;
