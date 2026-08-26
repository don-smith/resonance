import { describe, expect, it, vi } from "vitest";

import type { WorkspaceFilesSnapshot } from "@resonance/package-sdk";

import {
  editableMarkdownMount,
  type MarkdownEditorAdapter,
} from "../markdown-editor-mount.js";
import { createMarkdownEditorSession } from "../workspace-files-view.js";
import { editorIdentity, renderEditor } from "./markdown-editor.js";

class FakeElement {
  public hidden = false;
  public textContent: string | null = null;
  public dataset: Record<string, string> = {};
  public src = "";
  public children: FakeElement[] = [];
  public readonly ownerDocument = {
    createElement: () => new FakeElement(),
  } as unknown as Document;
  readonly #elements = new Map<string, FakeElement>();

  public register(selector: string): FakeElement {
    const element = new FakeElement();
    this.#elements.set(selector, element);
    return element;
  }

  public querySelector<T extends Element>(selector: string): T | null {
    return (this.#elements.get(selector) ?? null) as T | null;
  }

  public replaceChildren(...children: FakeElement[]): void {
    this.children = children;
  }
}

function editorRoot(): HTMLElement {
  const root = new FakeElement();
  for (const selector of [
    ".workspace-files-markdown-editor",
    ".workspace-files-placeholder",
    '[data-action="save-markdown"]',
    ".workspace-files-review-actions",
    ".workspace-files-editor-notice",
    ".workspace-files-editor-notice p",
    '[data-action="review-latest"]',
  ]) {
    root.register(selector);
  }
  return root as unknown as HTMLElement;
}

function snapshot(): WorkspaceFilesSnapshot {
  return {
    root: { state: "healthy" },
    entries: [
      {
        nodeId: "file",
        parentNodeId: null,
        name: "file.md",
        kind: "markdown",
        currentRevisionId: "revision",
        editable: true,
      },
    ],
    conflicts: [],
  };
}

describe("workspace-files editor reconciliation", () => {
  it("keeps an unchanged editor instance and remounts on identity change", () => {
    const root = editorRoot();
    const destroy = vi.fn();
    const editable = vi.fn(() =>
      editableMarkdownMount(
        { nodeId: "file", revisionId: "revision" },
        { destroy, getMarkdown: () => "draft" },
      ),
    );
    const adapter: MarkdownEditorAdapter = {
      editable,
      viewer: vi.fn(() => ({
        kind: "viewer" as const,
        instance: { destroy },
      })),
    };
    const firstSession = createMarkdownEditorSession(
      { nodeId: "file", revisionId: "revision", markdown: "base" },
      false,
    );
    const first = renderEditor(root, firstSession, snapshot(), null, adapter);
    const same = renderEditor(root, firstSession, snapshot(), first, adapter);

    expect(same).toBe(first);
    expect(editable).toHaveBeenCalledOnce();
    expect(destroy).not.toHaveBeenCalled();

    const nextSession = createMarkdownEditorSession(
      { nodeId: "file", revisionId: "next", markdown: "latest" },
      false,
    );
    expect(editorIdentity(firstSession)).not.toBe(editorIdentity(nextSession));
    renderEditor(root, nextSession, snapshot(), null, adapter);
    expect(editable).toHaveBeenCalledTimes(2);
  });
});
