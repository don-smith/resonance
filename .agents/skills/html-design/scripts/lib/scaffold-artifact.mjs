import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

import { loadPaletteAsset } from "./palette-assets.mjs";

async function exists(file) {
  try {
    await access(file);
    return true;
  } catch {
    return false;
  }
}

export async function scaffoldArtifact({
  root = process.cwd(),
  profile,
  palette,
  output,
  force = false,
} = {}) {
  if (!profile) throw new Error("A profile is required.");
  if (!output) throw new Error("An output path is required.");

  let profiles;
  const portableConfig = path.join(root, "config/profiles.json");
  if (await exists(portableConfig)) {
    profiles = JSON.parse(await readFile(portableConfig, "utf8"));
  } else {
    const { loadRecords } = await import("./records.mjs");
    const records = await loadRecords(root);
    if (records.diagnostics.length > 0) throw new Error("Cannot scaffold from invalid records.");
    profiles = records.profiles;
  }
  if (!profiles.some((record) => record.id === profile))
    throw new Error(`Unknown profile: ${profile}`);

  const destination = path.resolve(output);
  if (!force && (await exists(destination))) {
    throw new Error(`${destination} already exists. Pass --force to replace it.`);
  }

  const portableTemplate = path.join(root, `templates/${profile}.html`);
  const portable = await exists(portableTemplate);
  const templatePath = portable
    ? portableTemplate
    : path.join(root, `system/profiles/${profile}/template.html`);
  const cssPath = portable
    ? path.join(root, "assets/design-system.css")
    : path.join(root, "dist/design-system.css");
  const themePath = portable
    ? path.join(root, "assets/theme.js")
    : path.join(root, "dist/theme.js");
  const [template, css, themeScript, selectedPalette] = await Promise.all([
    readFile(templatePath, "utf8"),
    readFile(cssPath, "utf8"),
    readFile(themePath, "utf8"),
    loadPaletteAsset({ root, palette }),
  ]);
  const themeScriptTag = /\s*<script\s+src=["'][^"']*theme\.js["'][^>]*><\/script>/i;
  const paletteStylesheet = /\s*<link\s+rel=["']stylesheet["'][^>]*palette\.css["'][^>]*\/?>/i;
  const stylesheet = /\s*<link\s+rel=["']stylesheet["'][^>]*design-system\.css["'][^>]*\/?>/i;
  if (
    !themeScriptTag.test(template) ||
    !paletteStylesheet.test(template) ||
    !stylesheet.test(template)
  ) {
    throw new Error(
      `Profile template ${profile} must load theme.js, palette.css, and design-system.css.`,
    );
  }

  const html = template
    .replace("<html ", `<html data-ds-profile="${profile}" `)
    .replace(themeScriptTag, `\n    <script data-ds-theme>\n${themeScript.trim()}\n    </script>`)
    .replace(
      paletteStylesheet,
      `\n    <style data-ds-palette="${selectedPalette.id}">\n${selectedPalette.css.trim()}\n    </style>`,
    )
    .replace(stylesheet, `\n    <style data-ds-canonical>\n${css.trim()}\n    </style>`);

  await mkdir(path.dirname(destination), { recursive: true });
  await writeFile(destination, html);
  return {
    output: destination,
    profile,
    palette: selectedPalette.id,
    bytes: Buffer.byteLength(html),
    overwritten: force,
  };
}
