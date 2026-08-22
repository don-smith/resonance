import { spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  profileLaunches,
  removeProfileConfigurations,
  removeProfileConfigurationsSync,
  resetProfile,
  tauriArguments,
  viteArguments,
  writeProfileConfigurations,
} from "./desktop-profiles-lib.mjs";
import { loadLocalDevelopmentEnvironment } from "./local-development-environment.mjs";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const root = resolve(scriptDirectory, "..");
const cleanupScript = resolve(scriptDirectory, "desktop-profiles-cleanup.mjs");
loadLocalDevelopmentEnvironment(root);
const arguments_ = process.argv.slice(2);
// pnpm keeps the forwarding separator in argv for package scripts.
if (arguments_[0] === "--") arguments_.shift();

async function main() {
  if (arguments_[0] === "--reset") {
    if (arguments_.length !== 2)
      throw new Error("Use desktop:profiles -- --reset <profile-name>.");
    const removed = await resetProfile(arguments_[1], root);
    console.log(
      removed
        ? `Reset debug profile ${arguments_[1]}.`
        : `Debug profile ${arguments_[1]} does not exist.`,
    );
    return;
  }

  const profiles = profileLaunches(arguments_, root);
  await writeProfileConfigurations(profiles);
  let configurationsRemoved = false;
  const removeConfigurations = async () => {
    if (configurationsRemoved) return;
    configurationsRemoved = true;
    await removeProfileConfigurations(profiles);
  };
  process.once("exit", () => {
    if (!configurationsRemoved) removeProfileConfigurationsSync(profiles);
  });
  startCleanupSupervisor(profiles);

  const children = [];
  let stopping = false;
  const stop = async (exitCode) => {
    if (stopping) return;
    stopping = true;
    for (const child of children) child.kill("SIGTERM");
    await removeConfigurations();
    process.exitCode = exitCode;
  };
  const start = (command, childArguments, profile) => {
    const child = spawn(command, childArguments, {
      cwd: root,
      stdio: "inherit",
      env: {
        ...process.env,
        RESONANCE_DEBUG_PROFILE_NAME: profile.name,
      },
    });
    children.push(child);
    child.once("error", () => void stop(1));
    child.once("exit", (code, signal) => {
      if (!stopping) void stop(code ?? (signal ? 1 : 0));
    });
  };

  process.once("SIGINT", () => void stop(130));
  process.once("SIGTERM", () => void stop(143));
  for (const profile of profiles)
    start("pnpm", viteArguments(profile), profile);
  for (const profile of profiles)
    start("pnpm", tauriArguments(profile), profile);
}

function startCleanupSupervisor(profiles) {
  const cleanup = spawn(
    process.execPath,
    [
      cleanupScript,
      String(process.pid),
      ...profiles.map((profile) => profile.name),
    ],
    { cwd: root, detached: true, stdio: "ignore" },
  );
  cleanup.unref();
}

main().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
