#!/usr/bin/env node
import { fileURLToPath } from "node:url";

import { applyPalette } from "./lib/apply-palette.mjs";

const root = fileURLToPath(new URL("..", import.meta.url));
const args = process.argv.slice(2);
function value(name) {
  const index = args.indexOf(name);
  return index === -1 ? undefined : args[index + 1];
}

try {
  const result = await applyPalette({
    root,
    file: value("--file") ?? args.find((argument) => !argument.startsWith("--")),
    palette: value("--palette"),
  });
  console.log(`Applied ${result.palette}: ${result.output}`);
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
