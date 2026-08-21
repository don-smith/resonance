import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, relative, resolve } from "node:path";

import { describe, expect, it } from "vitest";

type VrsProblem = { file: string; message: string };

async function markdownFiles(directory: string): Promise<string[]> {
  const { readdir } = await import("node:fs/promises");
  const entries = await readdir(directory, { withFileTypes: true });
  const nested = await Promise.all(
    entries.map(async (entry) => {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) return markdownFiles(path);
      return entry.isFile() && entry.name.endsWith(".md") ? [path] : [];
    }),
  );
  return nested.flat();
}

export async function validateVrsTree(root: string): Promise<VrsProblem[]> {
  const files = await markdownFiles(root);
  const definitions = new Map<string, string>();
  const references: Array<{ id: string; file: string }> = [];
  const problems: VrsProblem[] = [];

  for (const file of files) {
    const content = await readFile(file, "utf8");
    const displayPath = relative(root, file);
    for (const match of content.matchAll(/\*\*(RS(?:\.[A-Z]+)*-(?:R|A|T|DQ)\d{2})\b/g)) {
      const id = match[1];
      if (definitions.has(id)) {
        problems.push({
          file: displayPath,
          message: `duplicate ID ${id}; first defined in ${definitions.get(id)}`,
        });
      } else {
        definitions.set(id, displayPath);
      }
    }
    for (const marker of content.matchAll(/`refines:\s*([^`]+)`/g)) {
      for (const id of marker[1].match(/RS(?:\.[A-Z]+)*-R\d{2}/g) ?? []) {
        references.push({ id, file: displayPath });
      }
    }
    if (file.endsWith("spec.md") && !/^## Status\s*\n\s*(Draft|Active|Stable)\.?\s*$/m.test(content)) {
      problems.push({ file: displayPath, message: "spec.md needs a valid ## Status line" });
    }
    if (file.includes("/.decisions/") && !/^Status:\s+accepted \([^\n]+\)\.$/m.test(content)) {
      problems.push({ file: displayPath, message: "decision needs an accepted Status line" });
    }
    if (file.includes("/.delta/") && !/^Status:\s+(open|closed \([^\n]+\))\.$/m.test(content)) {
      problems.push({ file: displayPath, message: "delta needs an open or closed Status line" });
    }
  }

  for (const reference of references) {
    if (!definitions.has(reference.id)) {
      problems.push({ file: reference.file, message: `unresolved refines ID ${reference.id}` });
    }
  }
  return problems;
}

async function fixtureTree(files: Record<string, string>): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "resonance-vrs-"));
  await Promise.all(
    Object.entries(files).map(async ([path, content]) => {
      const destination = join(root, path);
      await mkdir(resolve(destination, ".."), { recursive: true });
      await writeFile(destination, content);
    }),
  );
  return root;
}

describe("VRS structure", () => {
  it("accepts the authoritative context tree", async () => {
    await expect(validateVrsTree(resolve("context"))).resolves.toEqual([]);
  });

  it("rejects duplicate and unresolved IDs", async () => {
    const root = await fixtureTree({
      "requirements.md": "- **RS-R01 One.**\n- **RS-R01 Two.**\n- **RS-R02 Three.** `refines: RS-R99`\n",
    });
    try {
      const problems = await validateVrsTree(root);
      expect(problems.map((problem) => problem.message)).toContain("duplicate ID RS-R01; first defined in requirements.md");
      expect(problems.map((problem) => problem.message)).toContain("unresolved refines ID RS-R99");
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });

  it("rejects missing or invalid status lines", async () => {
    const root = await fixtureTree({
      "node/spec.md": "# Node\n\nStatus: Draft.\n",
      ".decisions/0001-test.md": "# Decision\n\nStatus: proposed.\n",
      ".delta/DELTA-001-test.md": "# Delta\n\nStatus: pending.\n",
    });
    try {
      expect(await validateVrsTree(root)).toHaveLength(3);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  });
});
