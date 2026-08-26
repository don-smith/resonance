import type { ManifestRole, SemanticCapability } from "@resonance/contracts";
import type { PackageContext, PackageInstance } from "@resonance/package-sdk";

export type BundledPackageManifest = Readonly<{
  id: string;
  name: string;
  nav: Readonly<{ label: string; icon: string }>;
  events: Readonly<{
    emits: readonly string[];
    consumes: readonly string[];
  }>;
  minRole: ManifestRole;
  capabilities?: readonly SemanticCapability[];
}>;

export type BundledPackageCatalogEntry = Readonly<{
  manifest: BundledPackageManifest;
  load(): Promise<unknown>;
}>;

export type PackageHostErrorStage =
  | "import"
  | "mount"
  | "activate"
  | "deactivate"
  | "dispose";

export type PackageHostOptions = {
  root: HTMLElement;
  catalog: readonly BundledPackageCatalogEntry[];
  createContext(manifest: BundledPackageManifest): PackageContext;
  onError?: (
    error: unknown,
    manifest: BundledPackageManifest,
    stage: PackageHostErrorStage,
  ) => void;
};

export type MountedRecord = {
  entry: BundledPackageCatalogEntry;
  root: HTMLElement;
  instance?: PackageInstance;
  mountPromise?: Promise<PackageInstance | null>;
  failed: boolean;
  disposed: boolean;
};
