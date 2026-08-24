export * from "./package-lifecycle.js";
export * from "./workspace-files-v1.js";

import type { PackageEvent, PackageEventAccess } from "./package-lifecycle.js";

/** The desktop supplies this transport. Package code never imports Tauri. */
export type PackageEventTransport = {
  emit(event: PackageEvent): Promise<void>;
  listen(
    eventName: string,
    handler: (event: PackageEvent) => void,
  ): Promise<() => void>;
};

export type PackageEventDeclarations = Readonly<{
  emits: readonly string[];
  consumes: readonly string[];
}>;

export class PackageSdk implements PackageEventAccess {
  readonly #emits: ReadonlySet<string>;
  readonly #consumes: ReadonlySet<string>;

  public constructor(
    private readonly transport: PackageEventTransport,
    declarations: PackageEventDeclarations,
  ) {
    this.#emits = new Set(declarations.emits);
    this.#consumes = new Set(declarations.consumes);
  }

  public emit(name: string, payload: unknown): Promise<void> {
    if (!this.#emits.has(name)) {
      return Promise.reject(
        new Error(`Undeclared package event emit: ${name}`),
      );
    }
    return this.transport.emit({ name, payload });
  }

  public listen(
    eventName: string,
    handler: (event: PackageEvent) => void,
  ): Promise<() => void> {
    if (!this.#consumes.has(eventName)) {
      return Promise.reject(
        new Error(`Undeclared package event subscription: ${eventName}`),
      );
    }
    return this.transport.listen(eventName, handler);
  }
}
