#!/usr/bin/env node
import { fileURLToPath } from "node:url";

import { refreshArtifactAssets } from "./lib/refresh-artifact-assets.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const args = process.argv.slice(2);
function value(name) {
  const index = args.indexOf(name);
  return index === -1 ? undefined : args[index + 1];
}

const file = args.find((argument, index) => {
  if (argument.startsWith("--")) return false;
  return args[index - 1] !== "--palette";
});

try {
  const result = await refreshArtifactAssets({ root, file, palette: value("--palette") });
  console.log(`Refreshed artifact assets with ${result.palette}: ${result.output}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
