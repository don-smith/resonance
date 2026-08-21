#!/usr/bin/env node
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
} from "node:fs";
import { basename, join } from "node:path";
import { parseArgs } from "node:util";

import { readReleaseConfiguration } from "./release-configuration.mjs";

export function collectSignedUpdateArtifacts({
  configuration,
  target,
  sourceDirectory,
  outputDirectory,
}) {
  const artifact = configuration.targets[target]?.artifact;
  if (!artifact) {
    throw new Error(
      `Release configuration has no artifact for target ${target}.`,
    );
  }

  const artifactSource = findUniqueFile(sourceDirectory, artifact);
  const signatureSource = findUniqueFile(sourceDirectory, `${artifact}.sig`);
  mkdirSync(outputDirectory, { recursive: true });

  const artifactOutput = join(outputDirectory, basename(artifact));
  const signatureOutput = `${artifactOutput}.sig`;
  copyFileSync(artifactSource, artifactOutput);
  copyFileSync(signatureSource, signatureOutput);
  return { artifact: artifactOutput, signature: signatureOutput };
}

function findUniqueFile(directory, expectedName) {
  if (!existsSync(directory)) {
    throw new Error(`Updater artifact directory does not exist: ${directory}`);
  }
  const matches = findFiles(directory, expectedName);
  if (matches.length !== 1) {
    throw new Error(
      `Expected exactly one ${expectedName} in ${directory}, found ${matches.length}.`,
    );
  }
  return matches[0];
}

function findFiles(directory, expectedName) {
  return readdirSync(directory).flatMap((entry) => {
    const path = join(directory, entry);
    if (statSync(path).isDirectory()) return findFiles(path, expectedName);
    return entry === expectedName ? [path] : [];
  });
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
      target: { type: "string" },
      source: { type: "string" },
      output: { type: "string" },
    },
  });
  if (!values.config || !values.target || !values.source || !values.output) {
    throw new Error(
      "Usage: collect-signed-update-artifacts.mjs --config <path> --target <target> --source <dir> --output <dir>",
    );
  }

  const collected = collectSignedUpdateArtifacts({
    configuration: readReleaseConfiguration(values.config),
    target: values.target,
    sourceDirectory: values.source,
    outputDirectory: values.output,
  });
  console.log(`Collected ${collected.artifact} and ${collected.signature}`);
}

if (
  process.argv[1] &&
  import.meta.url === new URL(`file://${process.argv[1]}`).href
) {
  main();
}
