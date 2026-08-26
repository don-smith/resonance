import type {
  PackageContentModule,
  PackageInstance,
} from "@resonance/package-sdk";

export function isContentModule(value: unknown): value is PackageContentModule {
  return (
    typeof value === "object" &&
    value !== null &&
    "mount" in value &&
    typeof value.mount === "function"
  );
}

export function isPackageInstance(value: unknown): value is PackageInstance {
  return (
    typeof value === "object" &&
    value !== null &&
    "activate" in value &&
    typeof value.activate === "function" &&
    "deactivate" in value &&
    typeof value.deactivate === "function" &&
    "dispose" in value &&
    typeof value.dispose === "function"
  );
}
