import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { loadEnvFile } from "node:process";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const localEnvironment = resolve(root, ".resonance", ".env");
if (existsSync(localEnvironment)) {
  loadEnvFile(localEnvironment);
}

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
