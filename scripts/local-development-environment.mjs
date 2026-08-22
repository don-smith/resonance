import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { loadEnvFile } from "node:process";

export function loadLocalDevelopmentEnvironment(root) {
  const environmentFile = resolve(root, ".resonance", ".env");
  if (existsSync(environmentFile)) {
    loadEnvFile(environmentFile);
  }
}
