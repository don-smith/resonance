import { writeFile } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";

import { describe, expect, it } from "vitest";

import {
  readReleaseConfiguration,
  validateReleaseConfiguration,
  validateSigningSecret,
} from "../../scripts/release-configuration.mjs";

const validConfiguration = {
  manifestEndpoint: "https://updates.example.invalid/latest.json",
  artifactBaseUrl: "https://downloads.example.invalid/releases/v0.1.0",
  publicKey: "ZWQyNTUxOS1wdWJsaWMta2V5LWZvci10ZXN0cy1vbmx5LW5vdC1hLXNlY3JldA==",
  targets: {
    "darwin-aarch64": { artifact: "Resonance_aarch64.app.tar.gz" },
    "darwin-x86_64": { artifact: "Resonance_x64.app.tar.gz" },
    "windows-x86_64": { artifact: "Resonance_x64-setup.nsis.zip" },
  },
};

describe("release configuration", () => {
  it("accepts a complete test-only static configuration", () => {
    expect(validateReleaseConfiguration(validConfiguration)).toEqual(
      validConfiguration,
    );
  });

  it("rejects absent, placeholder, and non-HTTPS configuration", async () => {
    expect(() =>
      readReleaseConfiguration(join(tmpdir(), "resonance-absent-release.json")),
    ).toThrow("Could not read release configuration");
    expect(() =>
      validateReleaseConfiguration({
        ...validConfiguration,
        publicKey: "REPLACE_WITH_A_PUBLIC_KEY",
      }),
    ).toThrow("non-placeholder");
    expect(() =>
      validateReleaseConfiguration({
        ...validConfiguration,
        manifestEndpoint: "http://updates.example.invalid/latest.json",
      }),
    ).toThrow("HTTPS");

    const malformedPath = join(
      tmpdir(),
      `resonance-release-${Date.now()}.json`,
    );
    await writeFile(malformedPath, "not json");
    expect(() => readReleaseConfiguration(malformedPath)).toThrow("malformed");
  });

  it("requires a CI-held signing secret when requested", () => {
    expect(() => validateSigningSecret(undefined)).toThrow(
      "TAURI_SIGNING_PRIVATE_KEY",
    );
    expect(() => validateSigningSecret("a".repeat(64))).not.toThrow();
  });
});
