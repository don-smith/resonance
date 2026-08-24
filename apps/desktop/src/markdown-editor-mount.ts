import {
  captureMarkdownDraft,
  type MarkdownEditorSession,
} from "./workspace-files-view.js";

export type EditableMarkdownInstance = {
  destroy(): void;
  getMarkdown(): string;
};

export type MarkdownViewerInstance = {
  destroy(): void;
};

export type MountedMarkdown =
  | { kind: "editable"; instance: EditableMarkdownInstance }
  | { kind: "viewer"; instance: MarkdownViewerInstance };

export function editableMarkdownMount(
  instance: EditableMarkdownInstance,
): MountedMarkdown {
  return { kind: "editable", instance };
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
  if (!session || session.readOnly || mounted?.kind !== "editable") {
    return session;
  }
  return captureMarkdownDraft(session, mounted.instance.getMarkdown());
}
