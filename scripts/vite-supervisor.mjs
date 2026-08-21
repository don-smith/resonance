import { createServer } from "vite";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const desktop = resolve(root, "apps/desktop");
const server = await createServer({
  configFile: resolve(desktop, "vite.config.ts"),
  root: desktop,
});

await server.listen();
server.printUrls();

let closing = false;
async function close(code) {
  if (closing) return;
  closing = true;
  await server.close();
  process.exit(code);
}

process.on("SIGINT", () => void close(130));
process.on("SIGTERM", () => void close(143));
process.on("SIGHUP", () => void close(129));
