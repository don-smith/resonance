import { emit, listen } from "@tauri-apps/api/event";

import {
  DeclaredPackageEvents,
  PackageMountError,
  packageDesignTokens,
  type PackageContext,
  type PackageEventTransport,
  type WorkspaceFilesV1,
} from "@resonance/package-sdk";
import {
  semanticCapabilityProperties,
  type SemanticCapability,
} from "@resonance/contracts";
import { WorkspaceFilesTauriAdapter } from "./workspace-files-tauri-adapter.js";
import type { BundledPackageManifest } from "./package-host.js";

let workspaceFilesAdapter: WorkspaceFilesTauriAdapter | null = null;

function workspaceFilesCapability(): WorkspaceFilesV1 {
  workspaceFilesAdapter ??= new WorkspaceFilesTauriAdapter();
  return workspaceFilesAdapter;
}

const tauriEvents: PackageEventTransport = {
  emit: (event) => emit(event.name, event.payload),
  listen: async (eventName, handler) =>
    listen<unknown>(eventName, (event) =>
      handler({ name: eventName, payload: event.payload }),
    ),
};

type PackageContextDependencies = Readonly<{
  events?: PackageEventTransport;
  workspaceFiles?: WorkspaceFilesV1;
}>;

export class UnsupportedPackageCapabilityError extends Error {
  public constructor(packageId: string, capability: SemanticCapability) {
    super(
      `Package ${packageId} declares unsupported capability ${capability}.`,
    );
    this.name = "UnsupportedPackageCapabilityError";
  }
}

export function createPackageContext(
  manifest: BundledPackageManifest,
  dependencies: PackageContextDependencies = {},
): PackageContext {
  const declared = manifest.capabilities ?? [];
  const workspaceFiles = dependencies.workspaceFiles
    ? () => dependencies.workspaceFiles
    : workspaceFilesCapability;
  const capabilities: Record<string, unknown> = {};
  for (const capability of declared) {
    if (capability !== "workspace-files:v1") {
      throw new PackageMountError(
        new UnsupportedPackageCapabilityError(manifest.id, capability).message,
        () => undefined,
      );
    }
    capabilities[semanticCapabilityProperties[capability]] = workspaceFiles();
  }

  const events = new DeclaredPackageEvents(
    dependencies.events ?? tauriEvents,
    manifest.events,
  );
  return Object.freeze({
    package: Object.freeze({ id: manifest.id, name: manifest.name }),
    events,
    designTokens: packageDesignTokens,
    capabilities,
  }) as PackageContext;
}

export async function disposePackageContexts(): Promise<void> {
  const adapter = workspaceFilesAdapter;
  workspaceFilesAdapter = null;
  await adapter?.dispose();
}
