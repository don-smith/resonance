import type { WorkspaceFilesPreview } from "@resonance/package-sdk";

import type { FileEntryView } from "../workspace-files-types.js";
import { childEntries } from "../workspace-files-view.js";
import { required } from "./dom.js";

export type PreviewOwner = {
  entry: FileEntryView;
  preview: WorkspaceFilesPreview | null;
};

export function clearPreview(root: HTMLElement): void {
  const host = required<HTMLElement>(root, ".workspace-files-preview");
  host.hidden = true;
  required<HTMLImageElement>(root, ".workspace-files-preview img").hidden =
    true;
  required<HTMLUListElement>(
    root,
    ".workspace-files-preview ul",
  ).replaceChildren();
}

export function renderPreview(
  root: HTMLElement,
  owner: PreviewOwner,
  entries: readonly FileEntryView[],
): string | null {
  const host = required<HTMLElement>(root, ".workspace-files-preview");
  const placeholder = required<HTMLElement>(
    root,
    ".workspace-files-placeholder",
  );
  const message = required<HTMLElement>(root, ".workspace-files-preview p");
  const image = required<HTMLImageElement>(
    root,
    ".workspace-files-preview img",
  );
  const children = required<HTMLUListElement>(
    root,
    ".workspace-files-preview ul",
  );
  host.hidden = false;
  placeholder.hidden = true;
  required<HTMLElement>(root, ".workspace-files-preview h3").textContent =
    owner.entry.name;
  image.hidden = true;
  children.hidden = true;
  children.replaceChildren();

  if (owner.entry.kind === "directory") {
    const directChildren = childEntries(entries, owner.entry.nodeId);
    message.textContent =
      directChildren.length === 0
        ? "Empty folder."
        : `${directChildren.length} workspace entries.`;
    children.hidden = false;
    for (const child of directChildren) {
      const item = root.ownerDocument.createElement("li");
      item.textContent = `${child.kind === "directory" ? "Folder" : "File"}: ${child.name}`;
      children.append(item);
    }
    return null;
  }

  if (owner.preview?.kind === "image") {
    const blob = new Blob([new Uint8Array(owner.preview.bytes)], {
      type: owner.preview.mimeType,
    });
    image.src = URL.createObjectURL(blob);
    image.alt = `Preview of ${owner.entry.name}`;
    image.hidden = false;
    message.textContent = `${owner.preview.mimeType} · ${owner.preview.byteLength} bytes`;
    return image.src;
  }
  message.textContent = owner.preview
    ? `Preview unavailable for ${owner.preview.mimeType} (${owner.preview.byteLength} bytes). Review this file outside Resonance.`
    : "Preview unavailable. Review this file outside Resonance.";
  return null;
}

export function revokePreviewUrl(url: string | null): void {
  if (url) URL.revokeObjectURL(url);
}
