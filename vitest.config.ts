import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: [
      "tests/**/*.test.ts",
      "apps/**/*.test.ts",
      "packages/**/*.test.ts",
      "scripts/**/*.test.ts",
    ],
    exclude: ["**/node_modules/**", "packages/contracts/templates/**"],
  },
});
