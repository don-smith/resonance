import {
  captureMarkdownDraft,
  type MarkdownEditorSession,
} from "./workspace-files-view.js";
import type { MarkdownRevisionView } from "./workspace-files-types.js";

export type EditableMarkdownInstance = {
  destroy(): void;
  getMarkdown(): string;
};

export type MarkdownViewerInstance = {
  destroy(): void;
};

export type MarkdownMountOwner = {
  nodeId: string;
  revisionId: string;
};

export type MarkdownEditorAdapter = {
  editable(host: HTMLElement, revision: MarkdownRevisionView): MountedMarkdown;
  viewer(host: HTMLElement, markdown: string): MountedMarkdown;
};

export type MountedMarkdown =
  | {
      kind: "editable";
      owner: MarkdownMountOwner;
      instance: EditableMarkdownInstance;
    }
  | { kind: "viewer"; instance: MarkdownViewerInstance };

export function editableMarkdownMount(
  owner: MarkdownMountOwner,
  instance: EditableMarkdownInstance,
): MountedMarkdown {
  return {
    kind: "editable",
    owner: { nodeId: owner.nodeId, revisionId: owner.revisionId },
    instance,
  };
}

export function viewerMarkdownMount(
  instance: MarkdownViewerInstance,
): MountedMarkdown {
  return { kind: "viewer", instance };
}

export function captureMountedMarkdownDraft(
  session: MarkdownEditorSession | null,
  mounted: MountedMarkdown | null,
): MarkdownEditorSession | null {
  if (
    !session ||
    session.readOnly ||
    mounted?.kind !== "editable" ||
    mounted.owner.nodeId !== session.loadedRevision.nodeId ||
    mounted.owner.revisionId !== session.loadedRevision.revisionId
  ) {
    return session;
  }
  return captureMarkdownDraft(session, mounted.instance.getMarkdown());
}
