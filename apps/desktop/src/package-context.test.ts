import { describe, expect, it, vi } from "vitest";

import type {
  PackageEventTransport,
  WorkspaceFilesV1,
} from "@resonance/package-sdk";
import { PackageMountError } from "@resonance/package-sdk";

import {
  createPackageContext,
  disposePackageContexts,
} from "./package-context.js";
import type { BundledPackageManifest } from "./package-host.js";

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

  it("does not make disposal depend on how many contexts were created", async () => {
    await expect(disposePackageContexts()).resolves.toBeUndefined();
    await expect(disposePackageContexts()).resolves.toBeUndefined();
  });
});
