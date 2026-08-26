import Editor, { type EditorCore } from "@toast-ui/editor";

import type { MarkdownRevisionView } from "./workspace-files-types.js";
import {
  editableMarkdownMount,
  viewerMarkdownMount,
  type MarkdownEditorAdapter,
} from "./markdown-editor-mount.js";

const toolbarItems = [
  ["heading", "bold", "italic", "strike"],
  ["ul", "ol", "task"],
  ["link", "quote", "code", "codeblock"],
];

function destroyable(instance: Pick<EditorCore, "destroy">): {
  destroy(): void;
} {
  return { destroy: () => instance.destroy() };
}

export const toastUiEditorAdapter: MarkdownEditorAdapter = {
  editable(host: HTMLElement, revision: MarkdownRevisionView) {
    const editor = new Editor({
      el: host,
      height: "32rem",
      initialEditType: "wysiwyg",
      initialValue: revision.markdown,
      hideModeSwitch: true,
      usageStatistics: false,
      toolbarItems,
    });
    return editableMarkdownMount(
      { nodeId: revision.nodeId, revisionId: revision.revisionId },
      {
        destroy: () => editor.destroy(),
        getMarkdown: () => editor.getMarkdown(),
      },
    );
  },

  viewer(host: HTMLElement, markdown: string) {
    const viewer = Editor.factory({
      el: host,
      viewer: true,
      theme: "dark",
      initialValue: markdown,
      usageStatistics: false,
    });
    return viewerMarkdownMount(destroyable(viewer));
  },
};
