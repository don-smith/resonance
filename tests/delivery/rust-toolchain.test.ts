import { readFile } from "node:fs/promises";

import { describe, expect, it } from "vitest";

async function pinnedToolchain() {
  const toolchain = await readFile("rust-toolchain.toml", "utf8");
  const channel = toolchain.match(/^channel\s*=\s*"([^"]+)"$/m)?.[1];
  if (!channel) throw new Error("rust-toolchain.toml has no pinned channel");
  return channel;
}

describe("Rust toolchain configuration", () => {
  it("keeps Cargo and CI aligned with the pinned local toolchain", async () => {
    const [toolchain, cargo, quality, release] = await Promise.all([
      pinnedToolchain(),
      readFile("Cargo.toml", "utf8"),
      readFile(".github/workflows/quality.yml", "utf8"),
      readFile(".github/workflows/release.yml", "utf8"),
    ]);

    expect(cargo).toContain(`rust-version = "${toolchain}"`);
    expect(quality).toContain(`toolchain: ${toolchain}`);
    expect(release).toContain(`toolchain: ${toolchain}`);
  });
});
