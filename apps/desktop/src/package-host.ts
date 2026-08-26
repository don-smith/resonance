import {
  PackageMountError,
  type PackageContentModule,
  type PackageContext,
  type PackageInstance,
} from "@resonance/package-sdk";

export type BundledPackageManifest = Readonly<{
  id: string;
  name: string;
  nav: Readonly<{ label: string; icon: string }>;
  events: Readonly<{
    emits: readonly string[];
    consumes: readonly string[];
  }>;
  capabilities?: readonly string[];
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

type MountedRecord = {
  entry: BundledPackageCatalogEntry;
  root: HTMLElement;
  instance?: PackageInstance;
  mountPromise?: Promise<PackageInstance | null>;
  failed: boolean;
  disposed: boolean;
};

function isContentModule(value: unknown): value is PackageContentModule {
  return (
    typeof value === "object" &&
    value !== null &&
    "mount" in value &&
    typeof value.mount === "function"
  );
}

function isPackageInstance(value: unknown): value is PackageInstance {
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

export class PackageHost {
  readonly #root: HTMLElement;
  readonly #entries: ReadonlyMap<string, BundledPackageCatalogEntry>;
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

  public get manifests(): readonly BundledPackageManifest[] {
    return [...this.#entries.values()].map(({ manifest }) => manifest);
  }

  public activate(packageId: string | null): Promise<void> {
    if (this.#disposed) return Promise.resolve();
    if (packageId !== null && !this.#entries.has(packageId)) {
      return Promise.reject(new Error(`Unknown bundled package: ${packageId}`));
    }

    this.#desiredPackageId = packageId;
    const version = ++this.#requestVersion;
    this.#transition = this.#transition.then(() =>
      this.#transitionTo(packageId, version),
    );
    return this.#transition;
  }

  public dispose(): Promise<void> {
    if (this.#disposePromise) return this.#disposePromise;
    this.#disposed = true;
    this.#desiredPackageId = null;
    this.#requestVersion += 1;
    this.#disposePromise = this.#transition.then(async () => {
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
            this.#onError(cleanupError, record.entry.manifest, "dispose");
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
    this.#onError(error, record.entry.manifest, stage);
    const document = record.root.ownerDocument;
    const region = document.createElement("div");
    region.setAttribute("role", "alert");
    region.setAttribute("data-package-error", stage);
    const heading = document.createElement("h2");
    heading.textContent = record.entry.manifest.name;
    const message = document.createElement("p");
    message.textContent = `${record.entry.manifest.name} could not ${
      stage === "import" ? "load" : stage
    }.`;
    region.append(heading, message);
    record.root.replaceChildren(region);
  }
}
