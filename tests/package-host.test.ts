import { describe, expect, it, vi } from "vitest";

import { PackageHost } from "../apps/desktop/src/package-host.js";
import {
  PackageMountError,
  packageDesignTokens,
  type PackageContext,
  type PackageInstance,
} from "@resonance/package-sdk";

class FakeElement {
  public readonly attributes = new Map<string, string>();
  public readonly children: FakeElement[] = [];
  public hidden = false;
  public textContent: string | null = null;
  public type = "";

  public constructor(
    public readonly tagName: string,
    public readonly ownerDocument: FakeDocument,
  ) {}

  public append(...children: FakeElement[]): void {
    this.children.push(...children);
  }

  public replaceChildren(...children: FakeElement[]): void {
    this.children.splice(0, this.children.length, ...children);
  }

  public setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  public removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  public getAttribute(name: string): string | null {
    return this.attributes.get(name) ?? null;
  }
}

class FakeDocument {
  public createElement(tagName: string): FakeElement {
    return new FakeElement(tagName, this);
  }
}

function element(document = new FakeDocument()): HTMLElement {
  return document.createElement("div") as unknown as HTMLElement;
}

function context(id: string): PackageContext {
  return {
    package: { id, name: id },
    events: {
      emit: async () => undefined,
      listen: async () => () => undefined,
    },
    designTokens: packageDesignTokens,
    capabilities: {},
  };
}

function entry(
  id: string,
  mount: (root: HTMLElement) => PackageInstance | Promise<PackageInstance>,
) {
  return {
    manifest: {
      id,
      name: id,
      nav: { label: id, icon: "box" },
      events: { emits: [], consumes: [] },
    },
    load: async () => ({ mount }),
  } as const;
}

function lifecycle(log: string[], id: string): PackageInstance {
  return {
    activate: () => {
      log.push(`${id}:activate`);
    },
    deactivate: () => {
      log.push(`${id}:deactivate`);
    },
    dispose: () => {
      log.push(`${id}:dispose`);
    },
  };
}

describe("package host", () => {
  it("mounts once, orders transitions, retains mounts, and disposes once", async () => {
    const log: string[] = [];
    const root = element();
    const host = new PackageHost({
      root,
      catalog: [
        entry("resonance.alpha", () => {
          log.push("alpha:mount");
          return lifecycle(log, "alpha");
        }),
        entry("resonance.beta", () => {
          log.push("beta:mount");
          return lifecycle(log, "beta");
        }),
      ],
      createContext: ({ id }) => context(id),
    });

    await host.activate("resonance.alpha");
    await host.activate("resonance.beta");
    await host.activate("resonance.alpha");

    expect(log).toEqual([
      "alpha:mount",
      "alpha:activate",
      "alpha:deactivate",
      "beta:mount",
      "beta:activate",
      "beta:deactivate",
      "alpha:activate",
    ]);
    const mountRoot = root as unknown as FakeElement;
    expect(mountRoot.children).toHaveLength(2);
    expect(mountRoot.children[0]?.hidden).toBe(false);
    expect(mountRoot.children[1]?.hidden).toBe(true);

    await host.dispose();
    await host.dispose();
    expect(log.filter((value) => value === "alpha:dispose")).toHaveLength(1);
    expect(log.filter((value) => value === "beta:dispose")).toHaveLength(1);
  });

  it("suppresses stale activation after an async load", async () => {
    let resolveAlpha: ((module: unknown) => void) | undefined;
    const alphaLoaded = new Promise<unknown>((resolve) => {
      resolveAlpha = resolve;
    });
    const alphaActivate = vi.fn();
    const betaActivate = vi.fn();
    const root = element();
    const host = new PackageHost({
      root,
      catalog: [
        {
          ...entry("resonance.alpha", () => ({
            activate: alphaActivate,
            deactivate: vi.fn(),
            dispose: vi.fn(),
          })),
          load: () => alphaLoaded,
        },
        entry("resonance.beta", () => ({
          activate: betaActivate,
          deactivate: vi.fn(),
          dispose: vi.fn(),
        })),
      ],
      createContext: ({ id }) => context(id),
    });

    const alphaTransition = host.activate("resonance.alpha");
    await Promise.resolve();
    const betaTransition = host.activate("resonance.beta");
    resolveAlpha?.({
      mount: () => ({
        activate: alphaActivate,
        deactivate: vi.fn(),
        dispose: vi.fn(),
      }),
    });
    await Promise.all([alphaTransition, betaTransition]);

    expect(alphaActivate).not.toHaveBeenCalled();
    expect(betaActivate).toHaveBeenCalledOnce();
  });

  it("contains load and activation failures to their package regions", async () => {
    const document = new FakeDocument();
    const shell = document.createElement("main");
    const onboarding = document.createElement("section");
    onboarding.textContent = "Create a workspace";
    const root = document.createElement("div");
    shell.append(onboarding, root);
    const errors: unknown[] = [];
    const host = new PackageHost({
      root: root as unknown as HTMLElement,
      catalog: [
        {
          manifest: {
            id: "resonance.broken",
            name: "Broken package",
            nav: { label: "Broken", icon: "x" },
            events: { emits: [], consumes: [] },
          },
          load: async () => {
            throw new Error("private import detail");
          },
        },
        entry("resonance.activation", () => ({
          activate: () => {
            throw new Error("private activation detail");
          },
          deactivate: vi.fn(),
          dispose: vi.fn(),
        })),
      ],
      createContext: ({ id }) => context(id),
      onError: (error) => errors.push(error),
    });

    await host.activate("resonance.broken");
    await host.activate("resonance.activation");

    expect(shell.children[0]).toBe(onboarding);
    expect(onboarding.textContent).toBe("Create a workspace");
    expect(root.children).toHaveLength(2);
    expect(root.children[0]?.children[0]?.getAttribute("role")).toBe("alert");
    expect(root.children[0]?.children[0]?.children[1]?.textContent).toBe(
      "Broken package could not load.",
    );
    expect(root.children[1]?.children[0]?.getAttribute("role")).toBe("alert");
    expect(errors).toHaveLength(2);
  });

  it("cleans a partial mount and successful instances exactly once", async () => {
    const partialCleanup = vi.fn();
    const dispose = vi.fn();
    const host = new PackageHost({
      root: element(),
      catalog: [
        entry("resonance.partial", () => {
          throw new PackageMountError("partial mount", partialCleanup);
        }),
        entry("resonance.complete", () => ({
          activate: vi.fn(),
          deactivate: vi.fn(),
          dispose,
        })),
      ],
      createContext: ({ id }) => context(id),
    });

    await host.activate("resonance.partial");
    await host.activate("resonance.complete");
    await host.dispose();
    await host.dispose();

    expect(partialCleanup).toHaveBeenCalledOnce();
    expect(dispose).toHaveBeenCalledOnce();
  });
});
