import { existsSync, rmSync } from "node:fs";
import {
  lstat,
  mkdir,
  readFile,
  rm,
  unlink,
  writeFile,
} from "node:fs/promises";
import { dirname, resolve } from "node:path";

const PROFILE_PATTERN = /^[a-z][a-z0-9-]{0,31}$/;
const PORTS = [1421, 1422];

export type DesktopProfile = Readonly<{
  name: string;
  port: number;
  devUrl: string;
  identifier: string;
  bundleName: string;
  configPath: string;
  runner?: string;
}>;

function errorCode(error: unknown): string | undefined {
  return (error as NodeJS.ErrnoException).code;
}

export function validateProfileName(name: unknown): string {
  if (
    typeof name !== "string" ||
    !PROFILE_PATTERN.test(name) ||
    name.endsWith("-")
  ) {
    throw new Error(
      "Profile names must use 1-32 lowercase letters, digits, or interior hyphens.",
    );
  }
  return name;
}

export function profileLaunch(
  name: string,
  port: number,
  root: string,
): DesktopProfile {
  const validatedName = validateProfileName(name);
  if (!Number.isSafeInteger(port) || port < 1024 || port > 65_535) {
    throw new Error("Profile ports must be integers from 1024 through 65535.");
  }
  return {
    name: validatedName,
    port,
    devUrl: `http://127.0.0.1:${port}`,
    identifier: `com.resonance.desktop.debug.${validatedName}`,
    bundleName: `Resonance Debug ${validatedName}`,
    configPath: resolve(
      root,
      ".resonance",
      "desktop-profiles",
      `${validatedName}.tauri.conf.json`,
    ),
    runner:
      process.platform === "darwin"
        ? resolve(root, "scripts", "macos-tauri-runner.sh")
        : undefined,
  };
}

export function profileLaunches(
  names: string[],
  root: string,
): DesktopProfile[] {
  if (!Array.isArray(names) || names.length !== 2) {
    throw new Error("desktop:profiles needs exactly two profile names.");
  }
  const validated = names.map(validateProfileName);
  if (new Set(validated).size !== validated.length) {
    throw new Error("desktop:profiles needs two distinct profile names.");
  }
  return validated.map((name, index) =>
    profileLaunch(name, PORTS[index], root),
  );
}

export function tauriArguments(profile: DesktopProfile): string[] {
  return [
    "--filter",
    "@resonance/desktop",
    "tauri",
    "dev",
    "--config",
    profile.configPath,
    ...(profile.runner ? ["--runner", profile.runner] : []),
    "--features",
    "debug-local-profiles",
    "--",
    "--",
    "--debug-profile",
    profile.name,
  ];
}

export function viteArguments(profile: DesktopProfile): string[] {
  return [
    "--filter",
    "@resonance/desktop",
    "exec",
    "vite",
    "--host",
    "127.0.0.1",
    "--port",
    String(profile.port),
    "--strictPort",
  ];
}

export async function writeProfileConfigurations(
  profiles: DesktopProfile[],
): Promise<void> {
  await Promise.all(
    profiles.map(async (profile) => {
      await mkdir(dirname(profile.configPath), { recursive: true });
      await writeFile(
        profile.configPath,
        `${JSON.stringify(
          {
            productName: profile.bundleName,
            identifier: profile.identifier,
            build: {
              beforeDevCommand: 'node -e "process.exit(0)"',
              devUrl: profile.devUrl,
            },
          },
          null,
          2,
        )}\n`,
        "utf8",
      );
    }),
  );
}

export async function removeProfileConfigurations(
  profiles: DesktopProfile[],
): Promise<void> {
  await Promise.all(
    profiles.map((profile) => rm(profile.configPath, { force: true })),
  );
}

export function removeProfileConfigurationsSync(
  profiles: DesktopProfile[],
): void {
  for (const profile of profiles) rmSync(profile.configPath, { force: true });
}

async function checkedDirectory(path: string): Promise<void> {
  const metadata = await lstat(path);
  if (!metadata.isDirectory() || metadata.isSymbolicLink()) {
    throw new Error("The debug profile root is not safe to reset.");
  }
}

export async function resetProfile(
  name: string,
  root: string,
): Promise<boolean> {
  validateProfileName(name);
  const resonanceRoot = resolve(root, ".resonance");
  const profilesRoot = resolve(resonanceRoot, "debug-profiles");
  if (!existsSync(profilesRoot)) return false;
  await checkedDirectory(resonanceRoot);
  await checkedDirectory(profilesRoot);
  const profileRoot = resolve(profilesRoot, name);
  if (dirname(profileRoot) !== profilesRoot || !existsSync(profileRoot))
    return false;
  await checkedDirectory(profileRoot);

  const lockPath = resolve(profileRoot, ".profile.lock");
  if (existsSync(lockPath)) {
    const pid = Number.parseInt((await readFile(lockPath, "utf8")).trim(), 10);
    if (!Number.isSafeInteger(pid) || pid <= 0) {
      throw new Error("The profile lock cannot be verified; refusing reset.");
    }
    try {
      process.kill(pid, 0);
      throw new Error(
        "The selected debug profile is active; close it before reset.",
      );
    } catch (error) {
      if (errorCode(error) !== "ESRCH") throw error;
      await unlink(lockPath);
    }
  }
  await rm(profileRoot, { recursive: true, force: false });
  return true;
}
