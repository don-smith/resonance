import { describe, expect, it, vi } from "vitest";

import type {
  ConversationsV1,
  PackageEventTransport,
  WorkspaceFilesV1,
} from "@resonance/package-sdk";
import { PackageMountError } from "@resonance/package-sdk";

import { bundledPackageCatalog } from "./generated/bundled-package-catalog.js";
import {
  createPackageContext,
  disposePackageContexts,
} from "./package-context.js";
import type { BundledPackageManifest } from "./package-host.js";

const conversations = {} as ConversationsV1;
const files = {} as WorkspaceFilesV1;

function manifest(
  capabilities: BundledPackageManifest["capabilities"] = ["workspace-files:v1"],
): BundledPackageManifest {
  return {
    id: "test.package",
    name: "Test package",
    nav: { label: "Test", icon: "test" },
    events: { emits: ["test:emitted"], consumes: ["test:received"] },
    minRole: "viewer",
    capabilities,
  };
}

function eventTransport(): PackageEventTransport {
  return {
    emit: vi.fn(async () => undefined),
    listen: vi.fn(async () => () => undefined),
  };
}

describe("package context authority", () => {
  it("provisions only declared supported capabilities and wraps package events", async () => {
    const events = eventTransport();
    const context = createPackageContext(manifest(), {
      events,
      workspaceFiles: files,
    });

    expect(context.capabilities.workspaceFilesV1).toBe(files);
    await context.events.emit("test:emitted", { ok: true });
    await expect(context.events.emit("not-declared", null)).rejects.toThrow(
      "Undeclared package event emit",
    );
    expect(events.emit).toHaveBeenCalledWith({
      name: "test:emitted",
      payload: { ok: true },
    });
  });

  it("provisions conversations only when the manifest declares it", () => {
    const context = createPackageContext(manifest(["conversations:v1"]), {
      conversations,
      events: eventTransport(),
    });
    expect(context.capabilities.conversationsV1).toBe(conversations);
    expect(context.capabilities.workspaceFilesV1).toBeUndefined();
  });

  it("rejects every declared capability without a provider", () => {
    expect(() =>
      createPackageContext(manifest(["documents:read"]), {
        events: eventTransport(),
        workspaceFiles: files,
      }),
    ).toThrow(PackageMountError);
    expect(() =>
      createPackageContext(manifest(["documents:read"]), {
        events: eventTransport(),
        workspaceFiles: files,
      }),
    ).toThrow("documents:read");
  });

  it("can create the context declared by the bundled reference package", () => {
    const reference = bundledPackageCatalog.find(
      ({ manifest }) => manifest.id === "resonance.reference",
    );
    if (!reference) throw new Error("reference package is missing");

    expect(
      createPackageContext(reference.manifest, { events: eventTransport() })
        .package.id,
    ).toBe("resonance.reference");
  });

  it("does not make disposal depend on how many contexts were created", async () => {
    await expect(disposePackageContexts()).resolves.toBeUndefined();
    await expect(disposePackageContexts()).resolves.toBeUndefined();
  });
});
