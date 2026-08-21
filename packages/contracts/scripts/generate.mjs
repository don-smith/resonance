import { mkdir, readFile, writeFile } from "node:fs/promises";
import { basename, dirname, resolve } from "node:path";

const argumentsByName = new Map(
  process.argv
    .slice(2)
    .flatMap((value, index, values) =>
      value.startsWith("--") && values[index + 1]
        ? [[value.slice(2), values[index + 1]]]
        : [],
    ),
);
const id = argumentsByName.get("id");
const output = argumentsByName.get("output");

if (
  !id ||
  !output ||
  !/^(resonance|[a-z][a-z0-9-]*)\.[a-z][a-z0-9-]*$/.test(id)
) {
  throw new Error(
    "Usage: pnpm generate -- --id <namespace.name> --output <directory>",
  );
}

const template = await readFile(
  resolve(import.meta.dirname, "../templates/manifest.v1.json"),
  "utf8",
);
const packageName = basename(id).replace(
  /(^|[-_])(\w)/g,
  (_, prefix, letter) => `${prefix}${letter.toUpperCase()}`,
);
const destination = resolve(output, "manifest.json");

await mkdir(dirname(destination), { recursive: true });
await writeFile(
  destination,
  template
    .replaceAll("__PACKAGE_ID__", id)
    .replaceAll("__PACKAGE_NAME__", packageName),
);
