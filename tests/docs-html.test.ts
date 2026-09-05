import { access, readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";

import { parse, type DefaultTreeAdapterMap } from "parse5";
import { describe, expect, it } from "vitest";

type Node = DefaultTreeAdapterMap["node"];
type Element = DefaultTreeAdapterMap["element"];

const documents = [
  "docs/html/index.html",
  "docs/html/workspace-files.html",
  "docs/html/conversations.html",
];

function elements(node: Node): Element[] {
  const children = "childNodes" in node ? node.childNodes : [];
  return children.flatMap((child) =>
    "tagName" in child ? [child, ...elements(child)] : elements(child),
  );
}

function attribute(element: Element, name: string): string | undefined {
  return element.attrs.find((value) => value.name === name)?.value;
}

describe("browser documentation", () => {
  for (const path of documents) {
    it(`${path} has valid structure and resolvable local links`, async () => {
      const source = await readFile(path, "utf8");
      const parseErrors: string[] = [];
      const document = parse(source, {
        onParseError: (error) =>
          parseErrors.push(
            `${error.code} at ${error.startLine}:${error.startCol}`,
          ),
      });
      expect(parseErrors).toEqual([]);

      const nodes = elements(document);
      const ids = nodes
        .map((node) => attribute(node, "id"))
        .filter((id): id is string => Boolean(id));
      expect(new Set(ids).size).toBe(ids.length);

      for (const link of nodes.filter((node) => node.tagName === "a")) {
        const href = attribute(link, "href");
        if (!href || /^(?:[a-z]+:|\/\/)/i.test(href)) continue;
        const [linkedPath, fragment] = href.split("#", 2);
        const destination = linkedPath
          ? resolve(dirname(path), linkedPath)
          : resolve(path);
        await expect(access(destination), href).resolves.toBeUndefined();
        if (fragment && (!linkedPath || destination === resolve(path))) {
          expect(ids, href).toContain(decodeURIComponent(fragment));
        }
      }
    });
  }
});
