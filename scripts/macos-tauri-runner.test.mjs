import { execFile } from "node:child_process";
import { readFile } from "node:fs/promises";
import { promisify } from "node:util";

import { describe, expect, it } from "vitest";

const execFileAsync = promisify(execFile);

const macosOnly = process.platform === "darwin" ? it : it.skip;

describe("macOS Tauri runner", () => {
  macosOnly(
    "launches the ordinary app without expanding an empty argument array",
    async () => {
      const runner = await readFile("scripts/macos-tauri-runner.sh", "utf8");

      expect(runner).toContain("if ((${#run_args[@]})); then");
      expect(runner).toContain('open -n -a "$bundle"');
    },
  );

  macosOnly(
    "recognizes a profile app process that has application arguments",
    async () => {
      const runner = await readFile("scripts/macos-tauri-runner.sh", "utf8");
      expect(runner).toContain('pgrep -f "^$binary( |$)"');

      const { stdout } = await execFileAsync("bash", [
        "-c",
        `binary='/tmp/Resonance Debug alice.app/Contents/MacOS/resonance-desktop'
       bash -c 'exec -a "$1" sleep 5' -- "$binary --debug-profile alice" & child=$!
       sleep 0.1
       matched=$(pgrep -f "^$binary( |$)" || true)
       kill "$child" 2>/dev/null || true
       test -n "$matched" && printf matched`,
      ]);
      expect(stdout).toBe("matched");
    },
  );
});
