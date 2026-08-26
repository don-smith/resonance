export const workspaceFilesV1Operations = [
  "snapshot",
  "select-root",
  "replace-root",
  "repair-root",
  "unbind-root",
  "open-markdown",
  "open-preview",
  "create-markdown",
  "replace-markdown",
  "resolve-conflict",
] as const;

export type WorkspaceFilesV1Operation =
  (typeof workspaceFilesV1Operations)[number];
