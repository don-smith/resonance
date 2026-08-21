#!/usr/bin/env node
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { basename, dirname, join } from "node:path";
import { parseArgs } from "node:util";

import { readReleaseConfiguration } from "./release-configuration.mjs";

export function generateUpdateManifest({
  configuration,
  artifactsDirectory,
  version,
  publishedAt,
}) {
  if (
    !/^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$/.test(version)
  ) {
    throw new Error("version must be a SemVer release string.");
  }

  const platforms = {};
  for (const [target, metadata] of Object.entries(configuration.targets)) {
    const artifactPath = join(artifactsDirectory, metadata.artifact);
    const signaturePath = `${artifactPath}.sig`;
    assertArtifactExists(artifactPath);
    const signature = readRequiredSignature(signaturePath);
    platforms[target] = {
      signature,
      url: new URL(
        basename(artifactPath),
        `${configuration.artifactBaseUrl.replace(/\/$/, "")}/`,
      ).href,
    };
  }

  return {
    version,
    notes: "Signed Resonance desktop release.",
    pub_date: publishedAt ?? new Date().toISOString(),
    platforms,
  };
}

function assertArtifactExists(path) {
  try {
    if (readFileSync(path).length === 0) throw new Error("empty artifact");
  } catch {
    throw new Error(`Missing or empty signed artifact: ${path}`);
  }
}

function readRequiredSignature(path) {
  let signature;
  try {
    signature = readFileSync(path, "utf8").trim();
  } catch {
    throw new Error(`Missing signature for signed artifact: ${path}`);
  }
  if (signature.length < 32 || /placeholder|replace|example/i.test(signature)) {
    throw new Error(`Malformed signature for signed artifact: ${path}`);
  }
  return signature;
}

function main() {
  const argumentsFromPackageScript = process.argv.slice(2);
  const args =
    argumentsFromPackageScript[0] === "--"
      ? argumentsFromPackageScript.slice(1)
      : argumentsFromPackageScript;
  const { values } = parseArgs({
    args,
    options: {
      config: { type: "string" },
      artifacts: { type: "string" },
      version: { type: "string" },
      output: { type: "string" },
      "published-at": { type: "string" },
    },
  });
  if (
    !values.config ||
    !values.artifacts ||
    !values.version ||
    !values.output
  ) {
    throw new Error(
      "Usage: generate-update-manifest.mjs --config <path> --artifacts <dir> --version <semver> --output <latest.json>",
    );
  }

  const manifest = generateUpdateManifest({
    configuration: readReleaseConfiguration(values.config),
    artifactsDirectory: values.artifacts,
    version: values.version,
    publishedAt: values["published-at"],
  });
  mkdirSync(dirname(values.output), { recursive: true });
  writeFileSync(values.output, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`Wrote static update manifest to ${values.output}`);
}

if (
  process.argv[1] &&
  import.meta.url === new URL(`file://${process.argv[1]}`).href
) {
  main();
}
