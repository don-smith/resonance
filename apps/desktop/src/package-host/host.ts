import {
  PackageMountError,
  type PackageInstance,
} from "@resonance/package-sdk";
import type { ManifestRole } from "@resonance/contracts";

import { reportPackageError } from "./error-region.js";
import { isContentModule, isPackageInstance } from "./guards.js";
import type {
  BundledPackageManifest,
  MountedRecord,
  PackageHostErrorStage,
  PackageHostOptions,
} from "./records.js";

const roleRank: Record<ManifestRole, number> = {
  viewer: 0,
  contributor: 1,
  developer: 2,
};

export function packageAllowedForRole(
  manifest: BundledPackageManifest,
  role: ManifestRole | null,
): boolean {
  return role !== null && roleRank[role] >= roleRank[manifest.minRole];
}

export class PackageHost {
  readonly #root: HTMLElement;
  readonly #entries: ReadonlyMap<string, PackageHostOptions["catalog"][number]>;
  readonly #createContext: PackageHostOptions["createContext"];
  readonly #onError: NonNullable<PackageHostOptions["onError"]>;
  readonly #records = new Map<string, MountedRecord>();
  #active: MountedRecord | null = null;
  #desiredPackageId: string | null = null;
  #requestVersion = 0;
  #transition: Promise<void> = Promise.resolve();
  #disposePromise: Promise<void> | null = null;
  #disposed = false;

  public constructor(options: PackageHostOptions) {
    this.#root = options.root;
    this.#entries = new Map(
      options.catalog.map((entry) => [entry.manifest.id, entry]),
    );
    if (this.#entries.size !== options.catalog.length) {
      throw new Error("The package host received duplicate package IDs.");
    }
    this.#createContext = options.createContext;
    this.#onError = options.onError ?? (() => undefined);
  }

  public manifestsForRole(
    role: ManifestRole | null,
  ): readonly BundledPackageManifest[] {
    return [...this.#entries.values()]
      .map(({ manifest }) => manifest)
      .filter((manifest) => packageAllowedForRole(manifest, role));
  }

  public activate(
    packageId: string | null,
    role: ManifestRole | null,
  ): Promise<void> {
    if (this.#disposed) return Promise.resolve();
    if (packageId !== null) {
      const entry = this.#entries.get(packageId);
      if (!entry) {
        return Promise.reject(
          new Error(`Unknown bundled package: ${packageId}`),
        );
      }
      if (!packageAllowedForRole(entry.manifest, role)) {
        return Promise.reject(
          new Error(`Package ${packageId} is unavailable for the active role.`),
        );
      }
    }

    this.#desiredPackageId = packageId;
    const version = ++this.#requestVersion;
    const requested = this.#transition
      .catch(() => undefined)
      .then(() => this.#transitionTo(packageId, version));
    this.#transition = requested.catch(() => undefined);
    return requested;
  }

  public dispose(): Promise<void> {
    if (this.#disposePromise) return this.#disposePromise;
    this.#disposed = true;
    this.#desiredPackageId = null;
    this.#requestVersion += 1;
    this.#disposePromise = this.#transition
      .catch(() => undefined)
      .then(async () => {
        if (this.#active) {
          await this.#deactivate(this.#active);
          this.#active = null;
        }
        await Promise.all(
          [...this.#records.values()].map((record) =>
            this.#disposeRecord(record),
          ),
        );
      });
    return this.#disposePromise;
  }

  async #transitionTo(
    packageId: string | null,
    version: number,
  ): Promise<void> {
    if (this.#disposed || version !== this.#requestVersion) return;

    for (const record of this.#records.values()) {
      if (record.entry.manifest.id !== packageId) record.root.hidden = true;
    }
    if (this.#active && this.#active.entry.manifest.id !== packageId) {
      const previous = this.#active;
      this.#active = null;
      await this.#deactivate(previous);
      previous.root.hidden = true;
    }
    if (
      this.#disposed ||
      version !== this.#requestVersion ||
      packageId === null
    ) {
      return;
    }
    if (this.#active?.entry.manifest.id === packageId) return;

    const record = this.#recordFor(packageId);
    record.root.hidden = false;
    const instance = await this.#mount(record);
    if (!instance || record.failed) return;
    if (
      this.#disposed ||
      version !== this.#requestVersion ||
      this.#desiredPackageId !== packageId
    ) {
      record.root.hidden = true;
      return;
    }

    try {
      await instance.activate();
    } catch (error) {
      record.failed = true;
      this.#report(record, "activate", error);
      return;
    }

    if (
      this.#disposed ||
      version !== this.#requestVersion ||
      this.#desiredPackageId !== packageId
    ) {
      await this.#deactivate(record);
      record.root.hidden = true;
      return;
    }
    this.#active = record;
  }

  #recordFor(packageId: string): MountedRecord {
    const existing = this.#records.get(packageId);
    if (existing) return existing;

    const entry = this.#entries.get(packageId);
    if (!entry) throw new Error(`Unknown bundled package: ${packageId}`);
    const root = this.#root.ownerDocument.createElement("section");
    root.hidden = true;
    root.setAttribute("data-package-id", packageId);
    root.setAttribute("aria-label", entry.manifest.nav.label);
    this.#root.append(root);
    const record: MountedRecord = {
      entry,
      root,
      failed: false,
      disposed: false,
    };
    this.#records.set(packageId, record);
    return record;
  }

  #mount(record: MountedRecord): Promise<PackageInstance | null> {
    if (record.instance) return Promise.resolve(record.instance);
    if (record.mountPromise) return record.mountPromise;

    record.mountPromise = (async () => {
      let loaded: unknown;
      try {
        loaded = await record.entry.load();
      } catch (error) {
        record.failed = true;
        this.#report(record, "import", error);
        return null;
      }
      if (!isContentModule(loaded)) {
        record.failed = true;
        this.#report(
          record,
          "import",
          new Error("The package entry does not export mount()."),
        );
        return null;
      }

      try {
        const instance = await loaded.mount(
          record.root,
          this.#createContext(record.entry.manifest),
        );
        if (!isPackageInstance(instance)) {
          throw new Error(
            "The package mount did not return a lifecycle instance.",
          );
        }
        record.instance = instance;
        return instance;
      } catch (error) {
        if (error instanceof PackageMountError) {
          try {
            await error.cleanup();
          } catch (cleanupError) {
            this.#report(record, "dispose", cleanupError);
          }
        }
        record.failed = true;
        this.#report(record, "mount", error);
        return null;
      }
    })();
    return record.mountPromise;
  }

  async #deactivate(record: MountedRecord): Promise<void> {
    if (!record.instance || record.failed) return;
    try {
      await record.instance.deactivate();
    } catch (error) {
      record.failed = true;
      this.#report(record, "deactivate", error);
    }
  }

  async #disposeRecord(record: MountedRecord): Promise<void> {
    if (record.disposed) return;
    record.disposed = true;
    const instance = record.instance;
    if (!instance) return;
    try {
      await instance.dispose();
    } catch (error) {
      this.#report(record, "dispose", error);
    }
  }

  #report(
    record: MountedRecord,
    stage: PackageHostErrorStage,
    error: unknown,
  ): void {
    reportPackageError(record, stage, error, this.#onError);
  }
}
