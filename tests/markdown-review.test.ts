import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import {
  captureMountedMarkdownDraft,
  editableMarkdownMount,
  viewerMarkdownMount,
} from "../apps/desktop/src/markdown-editor-mount.js";
import {
  captureMarkdownDraft,
  createMarkdownEditorSession,
  loadReviewedMarkdownRevision,
  returnToMarkdownDraft,
  reviewMarkdownRevision,
} from "../apps/desktop/src/workspace-files-view.js";

function contrastRatio(foreground: string, background: string): number {
  const luminance = (hex: string): number => {
    const channels = hex
      .slice(1)
      .match(/.{2}/g)!
      .map((channel) => Number.parseInt(channel, 16) / 255)
      .map((channel) =>
        channel <= 0.04045
          ? channel / 12.92
          : Math.pow((channel + 0.055) / 1.055, 2.4),
      );
    return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  };

  const lighter = Math.max(luminance(foreground), luminance(background));
  const darker = Math.min(luminance(foreground), luminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

describe("mounted Markdown review", () => {
  it("returns or loads from a mounted viewer without reading it as an editor", () => {
    const dirty = captureMarkdownDraft(
      createMarkdownEditorSession(
        {
          nodeId: "file",
          revisionId: "revision",
          markdown: "# Roadmap\n",
        },
        false,
      ),
      "# Roadmap\n\nUnsaved idea\n",
    );
    const reviewed = {
      nodeId: "file",
      revisionId: "next-revision",
      markdown: "# Roadmap\n\nPeer edit\n",
    };
    const reviewing = reviewMarkdownRevision(dirty, reviewed);
    const viewer = viewerMarkdownMount({
      destroy() {},
    });

    const returned = captureMountedMarkdownDraft(
      returnToMarkdownDraft(reviewing),
      viewer,
    );
    expect(returned?.draft).toBe(dirty.draft);
    expect(returned?.loadedRevision).toBe(dirty.loadedRevision);

    const loaded = captureMountedMarkdownDraft(
      loadReviewedMarkdownRevision(reviewing),
      viewer,
    );
    expect(loaded?.draft).toBe(reviewed.markdown);
    expect(loaded?.loadedRevision).toBe(reviewed);
  });

  it("captures a draft only from a mounted editable instance", () => {
    const session = createMarkdownEditorSession(
      {
        nodeId: "file",
        revisionId: "revision",
        markdown: "# Roadmap\n",
      },
      false,
    );
    const editor = editableMarkdownMount(session.loadedRevision, {
      destroy() {},
      getMarkdown: () => "# Roadmap\n\nUnsaved idea\n",
    });

    expect(captureMountedMarkdownDraft(session, editor)?.draft).toBe(
      "# Roadmap\n\nUnsaved idea\n",
    );
  });

  it("does not copy document A's mounted draft into document B", () => {
    const documentA = createMarkdownEditorSession(
      {
        nodeId: "document-a",
        revisionId: "revision-a",
        markdown: "# Document A\n",
      },
      false,
    );
    const documentB = createMarkdownEditorSession(
      {
        nodeId: "document-b",
        revisionId: "revision-b",
        markdown: "# Document B\n",
      },
      false,
    );
    const mountedDocumentA = editableMarkdownMount(documentA.loadedRevision, {
      destroy() {},
      getMarkdown: () => "# Document A\n\nUnsaved A draft\n",
    });

    expect(captureMountedMarkdownDraft(documentB, mountedDocumentA)).toBe(
      documentB,
    );
    expect(documentB.draft).toBe("# Document B\n");
  });

  it("does not copy a mounted draft into a newly created Markdown file", () => {
    const openDocument = createMarkdownEditorSession(
      {
        nodeId: "open-document",
        revisionId: "open-revision",
        markdown: "# Open document\n",
      },
      false,
    );
    const newDocument = createMarkdownEditorSession(
      {
        nodeId: "new-document",
        revisionId: "new-revision",
        markdown: "",
      },
      false,
    );
    const mountedOpenDocument = editableMarkdownMount(
      openDocument.loadedRevision,
      {
        destroy() {},
        getMarkdown: () => "# Open document\n\nUnsaved draft\n",
      },
    );

    expect(captureMountedMarkdownDraft(newDocument, mountedOpenDocument)).toBe(
      newDocument,
    );
    expect(newDocument.draft).toBe("");
  });

  it("uses a dark viewer theme with readable body text", async () => {
    const [source, styles] = await Promise.all([
      readFile(resolve("apps/desktop/src/main.ts"), "utf8"),
      readFile(resolve("apps/desktop/src/styles.css"), "utf8"),
    ]);
    const background = styles.match(
      /--markdown-review-background:\s*(#[0-9a-f]{6})/i,
    )?.[1];
    const foreground = styles.match(
      /--markdown-review-foreground:\s*(#[0-9a-f]{6})/i,
    )?.[1];

    expect(source).toContain(
      'import "@toast-ui/editor/dist/theme/toastui-editor-dark.css"',
    );
    expect(source).toMatch(/viewer:\s*true,[\s\S]*?theme:\s*"dark"/);
    expect(styles).toContain(
      "#markdown-editor.toastui-editor-dark .toastui-editor-contents p",
    );
    expect(background).toBeDefined();
    expect(foreground).toBeDefined();
    expect(contrastRatio(foreground!, background!)).toBeGreaterThanOrEqual(4.5);
  });
});
