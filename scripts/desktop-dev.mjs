import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { loadLocalDevelopmentEnvironment } from "./local-development-environment.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
loadLocalDevelopmentEnvironment(root);

const tauriArgs = ["--filter", "@resonance/desktop", "tauri", "dev"];

if (process.platform === "darwin") {
  tauriArgs.push("--runner", resolve(root, "scripts/macos-tauri-runner.sh"));
}

const child = spawn("pnpm", tauriArgs, {
  cwd: root,
  stdio: "inherit",
});
child.on("exit", (code, signal) => {
  process.exitCode = code ?? (signal ? 1 : 0);
});
