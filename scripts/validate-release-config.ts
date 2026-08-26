#!/usr/bin/env node
import { parseArgs } from "node:util";

import {
  readReleaseConfiguration,
  validateSigningSecret,
} from "./release-configuration.ts";

const argumentsFromPackageScript = process.argv.slice(2);
const args =
  argumentsFromPackageScript[0] === "--"
    ? argumentsFromPackageScript.slice(1)
    : argumentsFromPackageScript;
const { values } = parseArgs({
  args,
  options: {
    config: { type: "string" },
    "require-signing-secret": { type: "boolean", default: false },
  },
});

if (!values.config) {
  console.error(
    "Usage: validate-release-config.ts --config <path> [--require-signing-secret]",
  );
  process.exitCode = 2;
} else {
  try {
    readReleaseConfiguration(values.config);
    if (values["require-signing-secret"]) {
      validateSigningSecret(process.env.TAURI_SIGNING_PRIVATE_KEY);
    }
    console.log("Release configuration is provisioned and valid.");
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    console.error(`Release configuration validation failed: ${message}`);
    process.exitCode = 1;
  }
}
