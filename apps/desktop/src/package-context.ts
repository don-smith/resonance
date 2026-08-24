import { emit, listen } from "@tauri-apps/api/event";

import {
  PackageSdk,
  packageDesignTokens,
  type PackageContext,
} from "../../../packages/sdk/src/index.js";
import type { BundledPackageManifest } from "./package-host.js";

export function createPackageContext(
  manifest: BundledPackageManifest,
): PackageContext {
  const events = new PackageSdk(
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

  return Object.freeze({
    package: Object.freeze({ id: manifest.id, name: manifest.name }),
    events,
    designTokens: packageDesignTokens,
    capabilities: Object.freeze({}),
  });
}
