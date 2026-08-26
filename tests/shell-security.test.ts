import { readFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { resolve } from "node:path";

import postcss from "postcss";
import { describe, expect, it } from "vitest";

const requireFromWorkspaceFiles = createRequire(
  resolve("packages/workspace-files/package.json"),
);

describe("main webview policy", () => {
  it("limits inline styles to attributes required by Toast UI", async () => {
    const configuration = JSON.parse(
      await readFile("apps/desktop/src-tauri/tauri.conf.json", "utf8"),
    ) as { app: { security: { csp: string } } };
    const editor = await readFile(
      requireFromWorkspaceFiles.resolve("@toast-ui/editor"),
      "utf8",
    );

    expect(editor).toMatch(/\.style\.(?:display|overflowWrap)/);
    expect(configuration.app.security.csp).toContain("style-src-elem 'self'");
    expect(configuration.app.security.csp).toContain(
      "style-src-attr 'unsafe-inline'",
    );
    expect(configuration.app.security.csp).not.toContain(
      "style-src 'self' 'unsafe-inline'",
    );
  });

  it("does not apply shell element rules to package markup", async () => {
    const source = await readFile("apps/desktop/src/styles.css", "utf8");
    const selectors: string[] = [];
    postcss.parse(source).walkRules((rule) => {
      selectors.push(...postcss.list.comma(rule.selector));
    });

    for (const selector of [
      "h1",
      "h2",
      "h3",
      "form",
      "label",
      "input",
      "select",
      "button",
      ".workspace li",
    ]) {
      expect(selectors).not.toContain(selector);
    }
  });

  it("grants only explicit event and application permissions", async () => {
    const capability = JSON.parse(
      await readFile(
        "apps/desktop/src-tauri/capabilities/main-shell.json",
        "utf8",
      ),
    ) as { permissions: string[] };

    expect(capability.permissions).toEqual(
      expect.arrayContaining([
        "core:event:allow-emit",
        "core:event:allow-listen",
        "core:event:allow-unlisten",
      ]),
    );
    expect(capability.permissions).not.toContain("core:default");
    expect(capability.permissions).not.toContain("updater:default");
  });
});
