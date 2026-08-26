import { emit, listen } from "@tauri-apps/api/event";

import {
  DeclaredPackageEvents,
  packageDesignTokens,
  type PackageContext,
  type WorkspaceFilesV1,
} from "../../../packages/sdk/src/index.js";
import { WorkspaceFilesTauriAdapter } from "./workspace-files-tauri-adapter.js";
import type { BundledPackageManifest } from "./package-host.js";

let workspaceFilesAdapter: WorkspaceFilesTauriAdapter | null = null;

function workspaceFilesCapability(): WorkspaceFilesV1 {
  workspaceFilesAdapter ??= new WorkspaceFilesTauriAdapter();
  return workspaceFilesAdapter;
}

export function createPackageContext(
  manifest: BundledPackageManifest,
): PackageContext {
  const events = new DeclaredPackageEvents(
    {
      emit: (event) =>
        emit(event.name, {
          packageId: manifest.id,
          payload: event.payload,
        }),
      listen: async (eventName, handler) =>
        listen<unknown>(eventName, (event) =>
          handler({ name: eventName, payload: event.payload }),
        ),
    },
    manifest.events,
  );
  const capabilities = manifest.capabilities?.includes("workspace-files:v1")
    ? Object.freeze({ workspaceFilesV1: workspaceFilesCapability() })
    : Object.freeze({});

  return Object.freeze({
    package: Object.freeze({ id: manifest.id, name: manifest.name }),
    events,
    designTokens: packageDesignTokens,
    capabilities,
  });
}

export async function disposePackageContexts(): Promise<void> {
  const adapter = workspaceFilesAdapter;
  workspaceFilesAdapter = null;
  await adapter?.dispose();
}
