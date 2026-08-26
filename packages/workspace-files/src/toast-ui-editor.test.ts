import { describe, expect, it, vi } from "vitest";

import { toastUiEditorAdapter } from "./toast-ui-editor.js";

vi.mock("@toast-ui/editor", () => ({
  default: class FakeEditor {
    public destroy(): void {}

    public getMarkdown(): string {
      return "";
    }
  },
}));

describe("Toast UI editor adapter", () => {
  it("clears a review viewer theme before mounting an editable editor", () => {
    const remove = vi.fn();
    const host = {
      classList: { remove },
    } as unknown as HTMLElement;

    toastUiEditorAdapter.editable(host, {
      nodeId: "file",
      revisionId: "revision",
      markdown: "draft",
    });

    expect(remove).toHaveBeenCalledWith("toastui-editor-dark");
  });
});
