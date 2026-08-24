import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

import { loadPaletteAsset } from "./palette-assets.mjs";

const THEME_BLOCK = /<script\b(?=[^>]*\bdata-ds-theme(?=\s|=|\/?>))[^>]*>[\s\S]*?<\/script>/gi;
const PALETTE_BLOCK = /<style\b(?=[^>]*\bdata-ds-palette(?=\s|=|\/?>))[^>]*>[\s\S]*?<\/style>/gi;
const CANONICAL_BLOCK =
  /<style\b(?=[^>]*\bdata-ds-canonical(?=\s|=|\/?>))[^>]*>[\s\S]*?<\/style>/gi;
const THEME_REFERENCE =
  /<script\b(?=[^>]*\bsrc\s*=\s*["'](?!https?:|\/\/)[^"']*theme\.js(?:[?#][^"']*)?["'])[^>]*>\s*<\/script>/gi;
const PALETTE_REFERENCE =
  /<link\b(?=[^>]*\bhref\s*=\s*["'](?!https?:|\/\/)[^"']*palette\.css(?:[?#][^"']*)?["'])[^>]*\/?\s*>/gi;
const CANONICAL_REFERENCE =
  /<link\b(?=[^>]*\bhref\s*=\s*["'](?!https?:|\/\/)[^"']*design-system\.css(?:[?#][^"']*)?["'])[^>]*\/?\s*>/gi;

function matches(source, pattern) {
  pattern.lastIndex = 0;
  const found = [...source.matchAll(pattern)];
  pattern.lastIndex = 0;
  return found;
}

function replaceSingle(source, pattern, replacement, label) {
  const found = matches(source, pattern);
  if (found.length > 1) {
    throw new Error(`Cannot refresh an artifact with ${found.length} ${label} blocks.`);
  }
  return {
    source: found.length === 1 ? source.replace(pattern, () => replacement) : source,
    found: found.length === 1,
    match: found[0]?.[0],
  };
}

function insertBefore(source, patterns, block) {
  for (const pattern of patterns) {
    pattern.lastIndex = 0;
    const match = pattern.exec(source);
    pattern.lastIndex = 0;
    if (match) {
      return `${source.slice(0, match.index)}${block}\n    ${source.slice(match.index)}`;
    }
  }
  throw new Error("The artifact must contain a head element before its assets can be refreshed.");
}

function paletteIdFrom(block) {
  return block?.match(/\bdata-ds-palette\s*=\s*["']([^"']+)["']/i)?.[1];
}

export async function refreshArtifactAssets({ root = process.cwd(), file, palette } = {}) {
  if (!file) throw new Error("An artifact path is required.");
  const absoluteFile = path.resolve(file);
  const source = await readFile(absoluteFile, "utf8");
  const paletteMatches = matches(source, PALETTE_BLOCK);
  if (paletteMatches.length > 1) {
    throw new Error(
      `Cannot refresh an artifact with ${paletteMatches.length} marked palette blocks.`,
    );
  }
  const existingPalette = paletteIdFrom(paletteMatches[0]?.[0]);
  const selectedPalette = await loadPaletteAsset({
    root,
    palette: palette ?? existingPalette,
  });
  const [theme, canonical] = await Promise.all([
    readFile(path.join(root, "assets/theme.js"), "utf8").catch((error) => {
      if (error.code !== "ENOENT") throw error;
      return readFile(path.join(root, "dist/theme.js"), "utf8");
    }),
    readFile(path.join(root, "assets/design-system.css"), "utf8").catch((error) => {
      if (error.code !== "ENOENT") throw error;
      return readFile(path.join(root, "dist/design-system.css"), "utf8");
    }),
  ]);

  const themeBlock = `<script data-ds-theme>\n${theme.trim()}\n    </script>`;
  const paletteBlock = `<style data-ds-palette="${selectedPalette.id}">\n${selectedPalette.css.trim()}\n    </style>`;
  const canonicalBlock = `<style data-ds-canonical>\n${canonical.trim()}\n    </style>`;

  const refreshedTheme = replaceSingle(source, THEME_BLOCK, themeBlock, "marked theme");
  const refreshedPalette = replaceSingle(
    refreshedTheme.source,
    PALETTE_BLOCK,
    paletteBlock,
    "marked palette",
  );
  const refreshedCanonical = replaceSingle(
    refreshedPalette.source,
    CANONICAL_BLOCK,
    canonicalBlock,
    "marked canonical",
  );
  let updated = refreshedCanonical.source;
  const migrated = { theme: false, palette: false, canonical: false };

  if (!refreshedTheme.found) {
    const reference = replaceSingle(updated, THEME_REFERENCE, themeBlock, "theme reference");
    updated = reference.source;
    migrated.theme = reference.found;
    if (!reference.found) {
      updated = insertBefore(updated, [PALETTE_BLOCK, CANONICAL_BLOCK, /<\/head\s*>/i], themeBlock);
    }
  }
  if (!refreshedPalette.found) {
    const reference = replaceSingle(updated, PALETTE_REFERENCE, paletteBlock, "palette reference");
    updated = reference.source;
    migrated.palette = reference.found;
    if (!reference.found) {
      updated = insertBefore(updated, [CANONICAL_BLOCK, /<\/head\s*>/i], paletteBlock);
    }
  }
  if (!refreshedCanonical.found) {
    const reference = replaceSingle(
      updated,
      CANONICAL_REFERENCE,
      canonicalBlock,
      "canonical reference",
    );
    updated = reference.source;
    migrated.canonical = reference.found;
    if (!reference.found) {
      updated = insertBefore(updated, [/<\/head\s*>/i], canonicalBlock);
    }
  }

  await writeFile(absoluteFile, updated);
  return {
    output: absoluteFile,
    palette: selectedPalette.id,
    bytes: Buffer.byteLength(updated),
    inserted: {
      theme: !refreshedTheme.found && !migrated.theme,
      palette: !refreshedPalette.found && !migrated.palette,
      canonical: !refreshedCanonical.found && !migrated.canonical,
    },
    migrated,
  };
}
