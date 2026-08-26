import { readFile } from "node:fs/promises";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

import {
  captureMountedMarkdownDraft,
  editableMarkdownMount,
  viewerMarkdownMount,
} from "./markdown-editor-mount.js";
import {
  captureMarkdownDraft,
  createMarkdownEditorSession,
  loadReviewedMarkdownRevision,
  returnToMarkdownDraft,
  reviewMarkdownRevision,
} from "./workspace-files-view.js";

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

describe("workspace-files Markdown mount", () => {
  it("captures drafts only from the matching editable instance", () => {
    const session = createMarkdownEditorSession(
      { nodeId: "file", revisionId: "revision", markdown: "base\n" },
      false,
    );
    const editor = editableMarkdownMount(session.loadedRevision, {
      destroy() {},
      getMarkdown: () => "draft\n",
    });
    expect(captureMountedMarkdownDraft(session, editor)?.draft).toBe("draft\n");

    const other = createMarkdownEditorSession(
      { nodeId: "other", revisionId: "other-revision", markdown: "other\n" },
      false,
    );
    expect(captureMountedMarkdownDraft(other, editor)).toBe(other);
  });

  it("does not read a review viewer as an editor", () => {
    const dirty = captureMarkdownDraft(
      createMarkdownEditorSession(
        { nodeId: "file", revisionId: "revision", markdown: "base\n" },
        false,
      ),
      "draft\n",
    );
    const reviewing = reviewMarkdownRevision(dirty, {
      nodeId: "file",
      revisionId: "next",
      markdown: "peer\n",
    });
    const viewer = viewerMarkdownMount({ destroy() {} });
    expect(
      captureMountedMarkdownDraft(returnToMarkdownDraft(reviewing), viewer)
        ?.draft,
    ).toBe("draft\n");
    expect(
      captureMountedMarkdownDraft(
        loadReviewedMarkdownRevision(reviewing),
        viewer,
      )?.draft,
    ).toBe("peer\n");
  });

  it("keeps Toast UI and readable dark viewer styles inside the package", async () => {
    const styles = await readFile(
      resolve("packages/workspace-files/src/styles.css"),
      "utf8",
    );
    const background = styles.match(
      /--markdown-review-background:\s*(#[0-9a-f]{6})/i,
    )?.[1];
    const foreground = styles.match(
      /--markdown-review-foreground:\s*(#[0-9a-f]{6})/i,
    )?.[1];

    expect(styles).toMatch(
      /\[data-package-id="resonance\.workspace-files"\][\s\S]*?\.workspace-files-markdown-editor\.toastui-editor-dark[\s\S]*?\.toastui-editor-contents[\s\S]*?p,/,
    );
    expect(contrastRatio(foreground!, background!)).toBeGreaterThanOrEqual(4.5);
  });
});
