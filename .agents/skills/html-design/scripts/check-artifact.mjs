#!/usr/bin/env node
import { fileURLToPath } from "node:url";

import { checkArtifact, formatArtifactReport } from "./lib/check-artifact.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const args = process.argv.slice(2);
function value(name) {
  const index = args.indexOf(name);
  return index === -1 ? undefined : args[index + 1];
}

const file = args.find((argument, index) => {
  if (argument.startsWith("--")) return false;
  return !["--profile", "--format"].includes(args[index - 1]);
});

try {
  const report = await checkArtifact({ root, file, profile: value("--profile") });
  if (value("--format") === "json") console.log(JSON.stringify(report, null, 2));
  else console.log(formatArtifactReport(report));
  if (!report.ok) process.exitCode = 1;
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
