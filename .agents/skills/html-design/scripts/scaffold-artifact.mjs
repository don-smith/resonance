#!/usr/bin/env node
import { fileURLToPath } from "node:url";

import { scaffoldArtifact } from "./lib/scaffold-artifact.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const args = process.argv.slice(2);
function value(name) {
  const index = args.indexOf(name);
  return index === -1 ? undefined : args[index + 1];
}

try {
  const result = await scaffoldArtifact({
    root,
    profile: value("--profile"),
    palette: value("--palette"),
    output: value("--output") ?? args.find((argument) => !argument.startsWith("--")),
    force: args.includes("--force"),
  });
  console.log(`Scaffolded ${result.profile} with ${result.palette}: ${result.output}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
