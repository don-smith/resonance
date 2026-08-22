import { mkdtemp, mkdir, readFile, symlink, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import {
  profileLaunches,
  resetProfile,
  tauriArguments,
  validateProfileName,
  viteArguments,
  writeProfileConfigurations,
} from "./desktop-profiles-lib.mjs";

describe("desktop profile launcher contract", () => {
  it("creates deterministic isolated development launches", async () => {
    const root = await mkdtemp(join(tmpdir(), "resonance-profiles-"));
    const profiles = profileLaunches(["alice", "bob"], root);
    expect(profiles.map((profile) => profile.port)).toEqual([1421, 1422]);
    expect(new Set(profiles.map((profile) => profile.devUrl)).size).toBe(2);
    expect(new Set(profiles.map((profile) => profile.identifier)).size).toBe(2);
    expect(tauriArguments(profiles[0])).toContain("debug-local-profiles");
    if (process.platform === "darwin") {
      expect(tauriArguments(profiles[0])).toContain("--runner");
      expect(profiles[0].runner).toMatch(/macos-tauri-runner\.sh$/);
    }
    expect(tauriArguments(profiles[0]).slice(-3)).toEqual([
      "--",
      "--debug-profile",
      "alice",
    ]);
    expect(viteArguments(profiles[1])).toContain("--strictPort");

    await writeProfileConfigurations(profiles);
    const configuration = JSON.parse(
      await readFile(profiles[0].configPath, "utf8"),
    );
    expect(configuration.build.devUrl).toBe("http://127.0.0.1:1421");
    expect(configuration.identifier).toBe("com.resonance.desktop.debug.alice");
  });

  it("rejects unsafe, duplicate, and incomplete launch input", () => {
    expect(() => validateProfileName("../alice")).toThrow(/Profile names/);
    expect(() => profileLaunches(["alice", "alice"], process.cwd())).toThrow(
      /distinct/,
    );
    expect(() => profileLaunches(["alice"], process.cwd())).toThrow(
      /exactly two/,
    );
  });

  it("resets only a verified inactive direct child", async () => {
    const root = await mkdtemp(join(tmpdir(), "resonance-reset-"));
    const profileRoot = join(root, ".resonance", "debug-profiles", "alice");
    await mkdir(profileRoot, { recursive: true });
    await writeFile(join(profileRoot, "state"), "old");
    await writeFile(join(profileRoot, ".profile.lock"), `${process.pid}\n`);
    await expect(resetProfile("alice", root)).rejects.toThrow(/active/);
    await writeFile(join(profileRoot, ".profile.lock"), "999999\n");
    expect(await resetProfile("alice", root)).toBe(true);
    expect(await resetProfile("bob", root)).toBe(false);

    const profilesRoot = join(root, ".resonance", "debug-profiles");
    await symlink(root, join(profilesRoot, "bob"));
    await expect(resetProfile("bob", root)).rejects.toThrow(/not safe/);
  });
});
