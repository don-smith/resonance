import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

import { loadPaletteAsset } from "./palette-assets.mjs";

const PALETTE_BLOCK = /<style\s+data-ds-palette=["']([^"']+)["'][^>]*>[\s\S]*?<\/style>/i;

export async function applyPalette({ root = process.cwd(), file, palette } = {}) {
  if (!file) throw new Error("An artifact path is required.");
  if (!palette) throw new Error("A palette is required.");
  const absoluteFile = path.resolve(file);
  const [source, selectedPalette] = await Promise.all([
    readFile(absoluteFile, "utf8"),
    loadPaletteAsset({ root, palette }),
  ]);
  const match = source.match(PALETTE_BLOCK);
  if (!match) {
    throw new Error(`${absoluteFile} does not contain a marked data-ds-palette style block.`);
  }
  const replacement = `<style data-ds-palette="${selectedPalette.id}">\n${selectedPalette.css.trim()}\n    </style>`;
  const updated = source.replace(PALETTE_BLOCK, replacement);
  await writeFile(absoluteFile, updated);
  return {
    output: absoluteFile,
    previousPalette: match[1],
    palette: selectedPalette.id,
    bytes: Buffer.byteLength(updated),
  };
}
