import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  profileLaunches,
  removeProfileConfigurations,
} from "./desktop-profiles-lib.mjs";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const [parentProcessId, ...names] = process.argv.slice(2);
const parentPid = Number.parseInt(parentProcessId, 10);

if (!Number.isSafeInteger(parentPid) || parentPid <= 0) {
  process.exitCode = 1;
} else {
  const profiles = profileLaunches(names, root);
  while (processAlive(parentPid)) {
    await new Promise((resolveDelay) => setTimeout(resolveDelay, 100));
  }
  await removeProfileConfigurations(profiles);
}

function processAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (error) {
    return error.code !== "ESRCH";
  }
}
