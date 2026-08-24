import type { WorkspaceFilesV1 } from "./workspace-files-v1.js";

export type PackageIdentity = Readonly<{
  id: string;
  name: string;
}>;

export type PackageEvent = Readonly<{
  name: string;
  payload: unknown;
}>;

export type PackageEventAccess = {
  emit(name: string, payload: unknown): Promise<void>;
  listen(
    eventName: string,
    handler: (event: PackageEvent) => void,
  ): Promise<() => void>;
};

export const packageDesignTokens = Object.freeze({
  background: "--resonance-color-background",
  panel: "--resonance-color-panel",
  foreground: "--resonance-color-foreground",
  muted: "--resonance-color-muted",
  accent: "--resonance-color-accent",
  border: "--resonance-color-border",
  spacing: "--resonance-spacing",
  fontFamily: "--resonance-font-family",
});

export type PackageDesignTokens = typeof packageDesignTokens;

export type PackageContext = Readonly<{
  package: PackageIdentity;
  events: PackageEventAccess;
  designTokens: PackageDesignTokens;
  capabilities: Readonly<{
    workspaceFilesV1?: WorkspaceFilesV1;
  }>;
}>;

export type PackageInstance = {
  activate(): void | Promise<void>;
  deactivate(): void | Promise<void>;
  dispose(): void | Promise<void>;
};

export type PackageContentModule = {
  mount(
    root: HTMLElement,
    context: PackageContext,
  ): PackageInstance | Promise<PackageInstance>;
};

/**
 * A mount that allocated resources before failing can throw this error. The
 * host calls cleanup once. Other mount failures must release their own partial
 * resources before rejecting.
 */
export class PackageMountError extends Error {
  readonly #cleanup: () => void | Promise<void>;
  #cleaned = false;

  public constructor(
    message: string,
    cleanup: () => void | Promise<void>,
    options?: ErrorOptions,
  ) {
    super(message, options);
    this.name = "PackageMountError";
    this.#cleanup = cleanup;
  }

  public async cleanup(): Promise<void> {
    if (this.#cleaned) return;
    this.#cleaned = true;
    await this.#cleanup();
  }
}
