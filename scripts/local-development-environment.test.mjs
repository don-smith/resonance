import { mkdtemp, mkdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { loadLocalDevelopmentEnvironment } from "./local-development-environment.mjs";

describe("local development environment", () => {
  it("loads the ignored signing environment for every development launcher", async () => {
    const root = await mkdtemp(join(tmpdir(), "resonance-environment-"));
    const environmentDirectory = join(root, ".resonance");
    await mkdir(environmentDirectory);
    await writeFile(
      join(environmentDirectory, ".env"),
      "RESONANCE_TEST_LOCAL_ENVIRONMENT=loaded\n",
    );
    const previous = process.env.RESONANCE_TEST_LOCAL_ENVIRONMENT;
    try {
      delete process.env.RESONANCE_TEST_LOCAL_ENVIRONMENT;
      loadLocalDevelopmentEnvironment(root);
      expect(process.env.RESONANCE_TEST_LOCAL_ENVIRONMENT).toBe("loaded");
    } finally {
      if (previous === undefined)
        delete process.env.RESONANCE_TEST_LOCAL_ENVIRONMENT;
      else process.env.RESONANCE_TEST_LOCAL_ENVIRONMENT = previous;
    }
  });
});
