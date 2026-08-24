import { access, readFile, readdir } from "node:fs/promises";
import path from "node:path";

async function exists(file) {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}

export async function loadPaletteAsset({ root = process.cwd(), palette } = {}) {
  const portableConfig = path.join(root, "config/palette.json");
  const portable = await exists(portableConfig);
  const config = JSON.parse(
    await readFile(
      portable ? portableConfig : path.join(root, "design-system.config.json"),
      "utf8",
    ),
  );
  const defaultId = config.default ?? config.palette;
  const directory = portable
    ? path.join(root, "assets/palettes")
    : path.join(root, "dist/palettes");
  const available =
    config.available ??
    (await readdir(directory))
      .filter((file) => file.endsWith(".css"))
      .map((file) => file.slice(0, -4))
      .sort();
  const id = palette ?? defaultId;
  if (!available.includes(id)) throw new Error(`Unknown palette: ${id}`);

  const file = path.join(directory, `${id}.css`);
  try {
    return { id, css: await readFile(file, "utf8"), file, available, defaultId };
  } catch (error) {
    if (error.code === "ENOENT") throw new Error(`Palette asset is missing: ${file}`);
    throw error;
  }
}
