import { mkdir, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { collectSignedUpdateArtifacts } from "../../scripts/collect-signed-update-artifacts.mjs";

const configuration = {
  manifestEndpoint: "https://updates.example.invalid/latest.json",
  artifactBaseUrl: "https://updates.example.invalid/artifacts",
  publicKey: "ZWQyNTUxOS1wdWJsaWMta2V5LWZvci10ZXN0cy1vbmx5LW5vdC1hLXNlY3JldA==",
  targets: {
    "darwin-aarch64": { artifact: "Resonance_aarch64.app.tar.gz" },
    "darwin-x86_64": { artifact: "Resonance_x64.app.tar.gz" },
    "windows-x86_64": { artifact: "Resonance_x64-setup.nsis.zip" },
  },
};

async function fixtureDirectory() {
  const root = join(
    tmpdir(),
    `resonance-release-artifacts-${Date.now()}-${Math.random()}`,
  );
  const sourceDirectory = join(root, "bundle", "updater");
  const outputDirectory = join(root, "published");
  await mkdir(sourceDirectory, { recursive: true });
  await writeFile(
    join(sourceDirectory, "Resonance_aarch64.app.tar.gz"),
    "signed artifact",
  );
  await writeFile(
    join(sourceDirectory, "Resonance_aarch64.app.tar.gz.sig"),
    "a".repeat(88),
  );
  return { sourceDirectory: join(root, "bundle"), outputDirectory };
}

describe("signed updater artifact collection", () => {
  it("collects the configured artifact and signature from a nested Tauri bundle", async () => {
    const { sourceDirectory, outputDirectory } = await fixtureDirectory();
    const collected = collectSignedUpdateArtifacts({
      configuration,
      target: "darwin-aarch64",
      sourceDirectory,
      outputDirectory,
    });

    await expect(readFile(collected.artifact, "utf8")).resolves.toBe(
      "signed artifact",
    );
    await expect(readFile(collected.signature, "utf8")).resolves.toBe(
      "a".repeat(88),
    );
  });

  it("keeps the release template aligned with package-relative build output and Pages publication", async () => {
    const workflow = await readFile(".github/workflows/release.yml", "utf8");

    expect(workflow).toContain("--config src-tauri/tauri.release.conf.json");
    expect(workflow).toContain(
      "--source target/${{ matrix.target }}/release/bundle",
    );
    expect(workflow).toContain("cp release-artifacts/* pages/");
  });

  it("rejects a target without the configured signed artifact pair", async () => {
    const { sourceDirectory, outputDirectory } = await fixtureDirectory();
    await writeFile(
      join(sourceDirectory, "Resonance_aarch64.app.tar.gz.sig"),
      "",
    );
    await writeFile(join(sourceDirectory, "Resonance_aarch64.app.tar.gz"), "");

    expect(() =>
      collectSignedUpdateArtifacts({
        configuration,
        target: "windows-x86_64",
        sourceDirectory,
        outputDirectory,
      }),
    ).toThrow("Expected exactly one Resonance_x64-setup.nsis.zip");
  });
});
