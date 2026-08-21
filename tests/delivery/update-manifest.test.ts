import { mkdir, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { describe, expect, it } from "vitest";

import { generateUpdateManifest } from "../../scripts/generate-update-manifest.mjs";

const configuration = {
  manifestEndpoint: "https://updates.example.invalid/latest.json",
  artifactBaseUrl: "https://downloads.example.invalid/releases/v1.2.3",
  publicKey: "ZWQyNTUxOS1wdWJsaWMta2V5LWZvci10ZXN0cy1vbmx5LW5vdC1hLXNlY3JldA==",
  targets: {
    "darwin-aarch64": { artifact: "Resonance_aarch64.app.tar.gz" },
    "darwin-x86_64": { artifact: "Resonance_x64.app.tar.gz" },
    "windows-x86_64": { artifact: "Resonance_x64-setup.nsis.zip" },
  },
};

async function signedArtifacts() {
  const directory = join(
    tmpdir(),
    `resonance-artifacts-${Date.now()}-${Math.random()}`,
  );
  await mkdir(directory);
  await Promise.all(
    Object.values(configuration.targets).flatMap(({ artifact }) => [
      writeFile(join(directory, artifact), "signed build artifact"),
      writeFile(join(directory, `${artifact}.sig`), "a".repeat(88)),
    ]),
  );
  return directory;
}

describe("static update manifest generation", () => {
  it("creates a Tauri static manifest from signed artifacts", async () => {
    const artifactsDirectory = await signedArtifacts();
    const manifest = generateUpdateManifest({
      configuration,
      artifactsDirectory,
      version: "1.2.3",
      publishedAt: "2026-08-21T12:00:00.000Z",
    });

    expect(manifest).toMatchObject({
      version: "1.2.3",
      platforms: {
        "darwin-aarch64": {
          signature: "a".repeat(88),
          url: "https://downloads.example.invalid/releases/v1.2.3/Resonance_aarch64.app.tar.gz",
        },
      },
    });
  });

  it("rejects malformed artifact and signature fixtures", async () => {
    const artifactsDirectory = await signedArtifacts();
    await writeFile(
      join(artifactsDirectory, "Resonance_x64.app.tar.gz.sig"),
      "placeholder",
    );
    expect(() =>
      generateUpdateManifest({
        configuration,
        artifactsDirectory,
        version: "1.2.3",
      }),
    ).toThrow("Malformed signature");

    const cleanArtifacts = await signedArtifacts();
    await writeFile(join(cleanArtifacts, "Resonance_x64-setup.nsis.zip"), "");
    expect(() =>
      generateUpdateManifest({
        configuration,
        artifactsDirectory: cleanArtifacts,
        version: "1.2.3",
      }),
    ).toThrow("Missing or empty signed artifact");
  });
});
