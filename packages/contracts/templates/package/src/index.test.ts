import { describe, expect, it } from "vitest";

import { packageDesignTokens } from "@resonance/package-sdk";

import { mount } from "./index.js";

class FakeElement {
  public textContent = "";
  public readonly children: FakeElement[] = [];
  public readonly attributes = new Map<string, string>();
  public readonly ownerDocument = {
    createElement: () => new FakeElement(),
  };

  public append(child: FakeElement): void {
    this.children.push(child);
  }

  public setAttribute(name: string, value: string): void {
    this.attributes.set(name, value);
  }

  public removeAttribute(name: string): void {
    this.attributes.delete(name);
  }

  public replaceChildren(): void {
    this.children.length = 0;
  }
}

describe("__PACKAGE_ID__ lifecycle", () => {
  it("retains activation state and disposes package-owned content once", async () => {
    const root = new FakeElement();
    const instance = await mount(root as unknown as HTMLElement, {
      package: { id: "__PACKAGE_ID__", name: "__PACKAGE_NAME__" },
      events: {
        emit: async () => undefined,
        listen: async () => () => undefined,
      },
      designTokens: packageDesignTokens,
      capabilities: {},
    });

    expect(root.children[0]?.textContent).toBe("__PACKAGE_NAME__");
    instance.activate();
    expect(root.attributes.get("data-package-active")).toBe("true");
    instance.deactivate();
    expect(root.attributes.has("data-package-active")).toBe(false);
    instance.dispose();
    instance.dispose();
    expect(root.children).toEqual([]);
    instance.activate();
    expect(root.attributes.has("data-package-active")).toBe(false);
  });
});
