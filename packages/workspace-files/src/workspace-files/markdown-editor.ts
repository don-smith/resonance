import type {
  WorkspaceFilesSnapshot,
  WorkspaceFilesMarkdownRevision,
} from "@resonance/package-sdk";

import {
  captureMountedMarkdownDraft,
  type MarkdownEditorAdapter,
  type MountedMarkdown,
} from "../markdown-editor-mount.js";
import {
  markdownRevisionAwareness,
  type MarkdownEditorSession,
} from "../workspace-files-view.js";
import { required } from "./dom.js";

export function editorIdentity(session: MarkdownEditorSession): string {
  return [
    session.loadedRevision.nodeId,
    session.loadedRevision.revisionId,
    session.kind,
    session.kind === "review" ? session.reviewedRevision.revisionId : "",
  ].join("\u0000");
}

export function captureDraft(
  session: MarkdownEditorSession | null,
  mounted: MountedMarkdown | null,
): MarkdownEditorSession | null {
  return captureMountedMarkdownDraft(session, mounted);
}

export function renderEditor(
  root: HTMLElement,
  session: MarkdownEditorSession | null,
  snapshot: WorkspaceFilesSnapshot,
  mounted: MountedMarkdown | null,
  adapter: MarkdownEditorAdapter,
): MountedMarkdown | null {
  const host = required<HTMLElement>(root, ".workspace-files-markdown-editor");
  const placeholder = required<HTMLElement>(
    root,
    ".workspace-files-placeholder",
  );
  const save = required<HTMLButtonElement>(
    root,
    '[data-action="save-markdown"]',
  );
  const reviewActions = required<HTMLElement>(
    root,
    ".workspace-files-review-actions",
  );
  if (!session) {
    host.hidden = true;
    reviewActions.hidden = true;
    save.hidden = true;
    placeholder.hidden = false;
    return mounted;
  }

  host.hidden = false;
  placeholder.hidden = true;
  renderNotice(root, session, snapshot);
  const isReview = session.kind === "review";
  reviewActions.hidden = !isReview;
  save.hidden = isReview || session.readOnly;

  const expectedKind = isReview || session.readOnly ? "viewer" : "editable";
  if (mounted?.kind === expectedKind) return mounted;
  mounted?.instance.destroy();
  host.replaceChildren();
  if (isReview) {
    return adapter.viewer(host, session.reviewedRevision.markdown);
  }
  if (session.readOnly) return adapter.viewer(host, session.draft);
  return adapter.editable(host, session.loadedRevision);
}

function renderNotice(
  root: HTMLElement,
  session: MarkdownEditorSession,
  snapshot: WorkspaceFilesSnapshot,
): void {
  const notice = required<HTMLElement>(root, ".workspace-files-editor-notice");
  const message = required<HTMLElement>(
    root,
    ".workspace-files-editor-notice p",
  );
  const review = required<HTMLButtonElement>(
    root,
    '[data-action="review-latest"]',
  );
  notice.hidden = true;
  review.hidden = true;
  if (session.readOnly) return;
  const awareness = markdownRevisionAwareness(session, snapshot);
  if (awareness.state === "current") return;
  notice.hidden = false;
  if (awareness.state === "deleted") {
    message.textContent =
      "This file was deleted from the workspace. Your draft remains open.";
  } else if (awareness.state === "conflicted") {
    message.textContent = `The ${awareness.conflictKind} now affects this file. Your draft remains open.`;
  } else {
    message.textContent =
      "A newer workspace revision is available. Your draft remains open.";
    review.hidden = false;
    review.dataset.revisionId = awareness.currentRevisionId;
  }
}

export function reviewedRevision(
  session: MarkdownEditorSession,
): WorkspaceFilesMarkdownRevision | null {
  return session.kind === "review" ? session.reviewedRevision : null;
}
