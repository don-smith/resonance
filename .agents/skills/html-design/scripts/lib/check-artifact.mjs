import { readFile, readdir } from "node:fs/promises";
import path from "node:path";
import { parse } from "parse5";

import { loadPaletteAsset } from "./palette-assets.mjs";

const EXTERNAL_URL = /^(?:https?:)?\/\//i;
const URL_ATTRIBUTES = new Set(["action", "data", "href", "poster", "src", "srcset"]);
const FORBIDDEN_VOCABULARY =
  /\bReference Project(?:\s*2\.0)?\b|\bReferenceStore\b|reference-project-/i;
const REQUIRED_ASSET_RULES = new Set(["theme-asset", "palette-asset", "canonical-asset"]);
const SOURCE_PALETTE_ANCHORS = new Set([
  "#111816",
  "#223d31",
  "#35413b",
  "#607068",
  "#7cc39d",
  "#80c9a7",
  "#8fb9d2",
  "#e5bd65",
  "#ef9876",
  "#fffdf8",
]);

function attributes(node) {
  return Object.fromEntries(
    (node.attrs ?? []).map((attribute) => [attribute.name, attribute.value]),
  );
}

function walk(node, visit, parent = null) {
  visit(node, parent);
  for (const child of node.childNodes ?? []) walk(child, visit, node);
  if (node.content) walk(node.content, visit, node);
}

function descendants(node) {
  const result = [];
  walk(node, (child) => {
    if (child !== node) result.push(child);
  });
  return result;
}

function lineFor(node, fallback = 1) {
  return node?.sourceCodeLocation?.startLine ?? fallback;
}

function columnFor(node) {
  return node?.sourceCodeLocation?.startCol;
}

function textOf(node) {
  return (node.childNodes ?? [])
    .map((child) => (child.nodeName === "#text" ? child.value : textOf(child)))
    .join("");
}

function normalizedAsset(source, { allowTrailingCommas = false } = {}) {
  const input = source.replaceAll("\r\n", "\n").trim();
  let result = "";
  let quote;
  let comment;
  let escaped = false;

  for (let index = 0; index < input.length; index += 1) {
    const character = input[index];
    const next = input[index + 1];

    if (quote) {
      result += character;
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === quote) quote = undefined;
      continue;
    }
    if (comment === "line") {
      if (character === "\n") comment = undefined;
      else result += character;
      continue;
    }
    if (comment === "block") {
      result += character;
      if (character === "*" && next === "/") {
        result += next;
        index += 1;
        comment = undefined;
      }
      continue;
    }
    if (character === "/" && next === "/") {
      result += "//";
      index += 1;
      comment = "line";
      continue;
    }
    if (character === "/" && next === "*") {
      result += "/*";
      index += 1;
      comment = "block";
      continue;
    }
    if (['"', "'", "`"].includes(character)) {
      result += character;
      quote = character;
      continue;
    }
    if (/\s/.test(character)) {
      let nextIndex = index + 1;
      while (/\s/.test(input[nextIndex] ?? "")) nextIndex += 1;
      const previousCharacter = result.at(-1) ?? "";
      const nextCharacter = input[nextIndex] ?? "";
      if (/[\w$%-]/.test(previousCharacter) && /[\w$%-]/.test(nextCharacter)) result += " ";
      index = nextIndex - 1;
      continue;
    }
    if (allowTrailingCommas && character === ",") {
      let nextIndex = index + 1;
      while (/\s/.test(input[nextIndex] ?? "")) nextIndex += 1;
      if (/[\])}]/.test(input[nextIndex] ?? "")) continue;
    }
    result += character;
  }

  return result;
}

function relative(root, file) {
  const result = path.relative(root, file).split(path.sep).join("/");
  return result.startsWith("..") ? file : result;
}

function lineOfMatch(source, index) {
  return source.slice(0, index).split("\n").length;
}

async function readFirst(paths) {
  for (const file of paths) {
    try {
      return await readFile(file, "utf8");
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  throw new Error(`Required skill asset is missing: ${paths.join(" or ")}`);
}

async function readPaletteSources(root) {
  for (const directory of [
    path.join(root, "assets/palettes"),
    path.join(root, "dist/palettes"),
    path.join(root, "system/palettes"),
  ]) {
    try {
      const files = (await readdir(directory)).filter((file) => file.endsWith(".css")).sort();
      return (
        await Promise.all(files.map((file) => readFile(path.join(directory, file), "utf8")))
      ).join("\n");
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
    }
  }
  return readFirst([path.join(root, "assets/palette.css"), path.join(root, "dist/palette.css")]);
}

async function collectCss({ root, file, nodes, add }) {
  const sources = [];
  for (const node of nodes) {
    const attrs = attributes(node);
    if (node.tagName === "style") {
      sources.push({
        css: textOf(node),
        node,
        approved:
          Object.hasOwn(attrs, "data-ds-canonical") || Object.hasOwn(attrs, "data-ds-palette"),
      });
    }
    if (node.tagName === "link" && attrs.rel?.split(/\s+/).includes("stylesheet") && attrs.href) {
      if (EXTERNAL_URL.test(attrs.href)) continue;
      const cssPath = path.resolve(path.dirname(file), attrs.href.split(/[?#]/)[0]);
      try {
        sources.push({
          css: await readFile(cssPath, "utf8"),
          node,
          approved: [
            path.join(root, "assets/palette.css"),
            path.join(root, "assets/design-system.css"),
            path.join(root, "dist/palette.css"),
            path.join(root, "dist/design-system.css"),
          ].includes(cssPath),
        });
      } catch {
        add(
          "missing-local-asset",
          node,
          `Stylesheet does not exist: ${attrs.href}.`,
          "Correct the local path or inline the canonical design-system CSS.",
        );
      }
    }
  }
  return sources;
}

export function formatArtifactReport(report) {
  const failures = report.diagnostics.map((item) => {
    const location = `${item.path}:${item.line}${item.column ? `:${item.column}` : ""}`;
    return `${location}: ${item.rule}: ${item.message} Fix: ${item.fix}`;
  });
  const suppressions = report.suppressions.map(
    (item) => `${item.path}:${item.line}: suppressed ${item.rule}: ${item.reason}`,
  );
  return [...failures, ...suppressions, report.ok ? `Artifact is valid: ${report.path}` : ""]
    .filter(Boolean)
    .join("\n");
}

export async function checkArtifact({ root = process.cwd(), file, profile } = {}) {
  if (!file) throw new Error("An artifact path is required.");
  const absoluteFile = path.resolve(file);
  const reportPath = relative(root, absoluteFile);
  const source = await readFile(absoluteFile, "utf8");
  const document = parse(source, { sourceCodeLocationInfo: true });
  const nodes = [];
  walk(document, (node) => nodes.push(node));

  const diagnostics = [];
  const suppressions = [];
  const seenSuppressions = new Set();
  function add(rule, node, message, fix, explicitLine) {
    const attrs = attributes(node ?? {});
    const ignored = (attrs["data-ds-check-ignore"] ?? "").split(/\s+/).filter(Boolean);
    if (!REQUIRED_ASSET_RULES.has(rule) && ignored.includes(rule)) {
      const reason = attrs["data-ds-check-reason"]?.trim();
      if (reason) {
        const key = `${rule}:${lineFor(node)}:${reason}`;
        if (!seenSuppressions.has(key)) {
          suppressions.push({ path: reportPath, rule, reason, line: lineFor(node) });
          seenSuppressions.add(key);
        }
        return;
      }
      diagnostics.push({
        path: reportPath,
        rule: "invalid-suppression",
        message: `The ${rule} escape hatch has no reason.`,
        fix: "Add a specific data-ds-check-reason on the same element or remove the escape hatch.",
        line: lineFor(node),
        ...(columnFor(node) ? { column: columnFor(node) } : {}),
      });
      return;
    }
    diagnostics.push({
      path: reportPath,
      rule,
      message,
      fix,
      line: explicitLine ?? lineFor(node),
      ...(columnFor(node) ? { column: columnFor(node) } : {}),
    });
  }

  const byId = new Map();
  const internalLinks = [];
  const headings = [];
  const mains = [];
  const svgs = [];
  let htmlNode;
  let colorSchemeMeta;

  for (const node of nodes) {
    const attrs = attributes(node);
    if (node.tagName === "html") htmlNode = node;
    if (node.tagName === "main") mains.push(node);
    if (/^h[1-6]$/.test(node.tagName ?? "")) headings.push(node);
    if (node.tagName === "svg") svgs.push(node);
    if (node.tagName === "meta" && attrs.name?.toLowerCase() === "color-scheme") {
      colorSchemeMeta = node;
    }
    if (attrs.id) {
      if (byId.has(attrs.id)) {
        add(
          "duplicate-id",
          node,
          `ID ${attrs.id} is already used on line ${lineFor(byId.get(attrs.id))}.`,
          "Give every element a unique ID and update links or aria references to match.",
        );
      } else byId.set(attrs.id, node);
    }
    if (attrs.href?.startsWith("#") && attrs.href.length > 1) {
      internalLinks.push({ node, target: decodeURIComponent(attrs.href.slice(1)) });
    }
    for (const [name, value] of Object.entries(attrs)) {
      if (
        URL_ATTRIBUTES.has(name) &&
        (EXTERNAL_URL.test(value) || /(?:^|\s)https?:\/\//i.test(value))
      ) {
        add(
          "external-request",
          node,
          `${name} requests a remote resource: ${value}.`,
          "Copy the asset into the artifact or remove the request.",
        );
      }
    }
  }

  for (const { node, target } of internalLinks) {
    if (!byId.has(target)) {
      add(
        "broken-internal-link",
        node,
        `Internal link points to missing #${target}.`,
        "Add the target ID or change the link to an existing section.",
      );
    }
  }

  const h1s = headings.filter((node) => node.tagName === "h1");
  if (mains.length !== 1 || h1s.length !== 1) {
    add(
      "document-structure",
      mains[1] ?? h1s[1] ?? mains[0] ?? h1s[0] ?? htmlNode,
      `Expected one main landmark and one h1; found ${mains.length} main landmarks and ${h1s.length} h1 elements.`,
      "Keep one main landmark and one page-level h1, then use sections for the remaining hierarchy.",
    );
  }
  let previousLevel = 0;
  for (const heading of headings) {
    const level = Number(heading.tagName.slice(1));
    if (previousLevel && level > previousLevel + 1) {
      add(
        "document-structure",
        heading,
        `Heading level jumps from h${previousLevel} to h${level}.`,
        `Use h${previousLevel + 1} or add the missing intermediate section heading.`,
      );
    }
    previousLevel = level;
  }

  const htmlAttrs = attributes(htmlNode ?? {});
  const theme = htmlAttrs["data-theme"];
  const schemes = attributes(colorSchemeMeta ?? {}).content?.toLowerCase() ?? "";
  if (
    !["system", "light", "dark"].includes(theme) ||
    !schemes.includes("light") ||
    !schemes.includes("dark")
  ) {
    add(
      "theme-metadata",
      htmlNode,
      "The artifact must declare a valid data-theme and light/dark color-scheme metadata.",
      'Set data-theme="system" on html and add <meta name="color-scheme" content="light dark">.',
    );
  }

  const themeScripts = nodes.filter(
    (node) => node.tagName === "script" && Object.hasOwn(attributes(node), "data-ds-theme"),
  );
  const paletteStyles = nodes.filter(
    (node) => node.tagName === "style" && Object.hasOwn(attributes(node), "data-ds-palette"),
  );
  const canonicalStyles = nodes.filter(
    (node) => node.tagName === "style" && Object.hasOwn(attributes(node), "data-ds-canonical"),
  );
  const [currentTheme, currentCanonical] = await Promise.all([
    readFirst([path.join(root, "assets/theme.js"), path.join(root, "dist/theme.js")]),
    readFirst([
      path.join(root, "assets/design-system.css"),
      path.join(root, "dist/design-system.css"),
    ]),
  ]);

  if (themeScripts.length !== 1) {
    add(
      "theme-asset",
      themeScripts[1] ?? themeScripts[0] ?? htmlNode,
      `Expected one marked theme script; found ${themeScripts.length}.`,
      "Run scripts/refresh-artifact-assets.mjs to install the current data-ds-theme script.",
    );
  } else if (
    normalizedAsset(textOf(themeScripts[0]), { allowTrailingCommas: true }) !==
    normalizedAsset(currentTheme, { allowTrailingCommas: true })
  ) {
    add(
      "theme-asset",
      themeScripts[0],
      "The marked theme script does not match the current design-system asset.",
      "Run scripts/refresh-artifact-assets.mjs to replace the stale data-ds-theme script.",
    );
  }

  if (paletteStyles.length !== 1) {
    add(
      "palette-asset",
      paletteStyles[1] ?? paletteStyles[0] ?? htmlNode,
      `Expected one marked named palette; found ${paletteStyles.length}.`,
      "Run scripts/refresh-artifact-assets.mjs with --palette to install a current named palette.",
    );
  } else {
    const paletteNode = paletteStyles[0];
    const paletteId = attributes(paletteNode)["data-ds-palette"];
    try {
      const currentPalette = await loadPaletteAsset({ root, palette: paletteId });
      if (normalizedAsset(textOf(paletteNode)) !== normalizedAsset(currentPalette.css)) {
        add(
          "palette-asset",
          paletteNode,
          `The marked ${paletteId} palette does not match the current design-system asset.`,
          "Run scripts/refresh-artifact-assets.mjs to replace the stale palette block.",
        );
      }
    } catch (error) {
      add(
        "palette-asset",
        paletteNode,
        error.message,
        "Choose a named palette from config/palette.json and run scripts/refresh-artifact-assets.mjs with --palette.",
      );
    }
  }

  if (canonicalStyles.length !== 1) {
    add(
      "canonical-asset",
      canonicalStyles[1] ?? canonicalStyles[0] ?? htmlNode,
      `Expected one marked canonical stylesheet; found ${canonicalStyles.length}.`,
      "Run scripts/refresh-artifact-assets.mjs to install the current data-ds-canonical stylesheet.",
    );
  } else if (normalizedAsset(textOf(canonicalStyles[0])) !== normalizedAsset(currentCanonical)) {
    add(
      "canonical-asset",
      canonicalStyles[0],
      "The marked canonical stylesheet does not match the current design-system asset.",
      "Run scripts/refresh-artifact-assets.mjs to replace the stale data-ds-canonical stylesheet.",
    );
  }

  for (const svg of svgs) {
    const attrs = attributes(svg);
    const children = descendants(svg);
    const childIds = new Map(
      children.map((node) => [attributes(node).id, node]).filter(([id]) => Boolean(id)),
    );
    const labels = (attrs["aria-labelledby"] ?? "").split(/\s+/).filter(Boolean);
    const title = labels.find((id) => childIds.get(id)?.tagName === "title");
    const description = labels.find((id) => childIds.get(id)?.tagName === "desc");
    if (!attrs.viewBox || attrs.role !== "img" || !title || !description) {
      add(
        "svg-accessibility",
        svg,
        "SVG diagrams need a viewBox, role=img, and aria-labelledby references to a title and description.",
        'Add a viewBox, role="img", child title and desc IDs, and reference both IDs from aria-labelledby.',
      );
    }
  }

  const cssSources = await collectCss({ root, file: absoluteFile, nodes, add });
  const canonicalTokens = new Set();
  const [tokenSource, paletteSource] = await Promise.all([
    readFirst([
      path.join(root, "assets/tokens.css"),
      path.join(root, "system/foundations/tokens.css"),
    ]),
    readPaletteSources(root),
  ]);
  for (const match of `${tokenSource}\n${paletteSource}`.matchAll(/(--ds-[\w-]+)\s*:/g)) {
    canonicalTokens.add(match[1]);
  }
  const sourcePalette = new Set([
    ...SOURCE_PALETTE_ANCHORS,
    ...[...paletteSource.matchAll(/#[0-9a-f]{6}\b/gi)].map((match) => match[0].toLowerCase()),
  ]);

  for (const { css, node, approved } of cssSources) {
    if (
      /@import\s+(?:url\()?\s*["']?(?:https?:)?\/\//i.test(css) ||
      /url\(\s*["']?(?:https?:)?\/\//i.test(css)
    ) {
      add(
        "external-request",
        node,
        "CSS imports or references a remote resource.",
        "Inline the resource or replace it with a local artifact-relative asset.",
      );
    }
    if (!approved) {
      const usedTokens = new Set([
        ...[...css.matchAll(/var\(\s*(--ds-[\w-]+)/g)].map((match) => match[1]),
        ...[...css.matchAll(/(--ds-[\w-]+)\s*:/g)].map((match) => match[1]),
      ]);
      for (const token of usedTokens) {
        if (!canonicalTokens.has(token)) {
          add(
            "invalid-token",
            node,
            `Unknown design-system token: ${token}.`,
            "Use a token declared by the canonical palette or foundations, or use a content-specific custom property without the --ds- prefix.",
          );
        }
      }
      for (const match of css.matchAll(/#[0-9a-f]{6}\b/gi)) {
        if (sourcePalette.has(match[0].toLowerCase())) {
          add(
            "source-palette",
            node,
            `Source palette literal ${match[0]} appears outside an approved canonical asset.`,
            "Use a canonical semantic token, or add a reasoned inline escape hatch for required content branding.",
          );
        }
      }
    }
  }

  const vocabulary = source.match(FORBIDDEN_VOCABULARY);
  if (vocabulary) {
    add(
      "forbidden-vocabulary",
      htmlNode,
      `Source-specific vocabulary appears in the artifact: ${vocabulary[0]}.`,
      "Replace source-domain names with terms from the artifact's own subject matter.",
      lineOfMatch(source, vocabulary.index),
    );
  }

  if (profile) {
    let profiles;
    try {
      profiles = JSON.parse(await readFile(path.join(root, "config/profiles.json"), "utf8"));
    } catch (error) {
      if (error.code !== "ENOENT") throw error;
      const { loadRecords } = await import("./records.mjs");
      profiles = (await loadRecords(root)).profiles;
    }
    const profileRecord = profiles.find((record) => record.id === profile);
    if (!profileRecord) throw new Error(`Unknown profile: ${profile}`);
    const css = cssSources.map((item) => item.css).join("\n");
    if (!/@media\s*\([^)]*max-width/i.test(css)) {
      add(
        "profile-responsive",
        htmlNode,
        `Profile ${profile} requires explicit narrow-screen behavior.`,
        "Load or inline canonical profile CSS with a max-width media query.",
      );
    }
    if (
      profileRecord.print !==
        "not required unless the application has an explicit reporting task" &&
      !/@media\s+print/i.test(css)
    ) {
      add(
        "profile-print",
        htmlNode,
        `Profile ${profile} requires print behavior.`,
        "Load or inline the canonical print CSS and verify the printed document.",
      );
    }
  }

  diagnostics.sort((a, b) => a.line - b.line || a.rule.localeCompare(b.rule));
  suppressions.sort((a, b) => a.line - b.line || a.rule.localeCompare(b.rule));
  return {
    ok: diagnostics.length === 0,
    path: reportPath,
    profile: profile ?? null,
    diagnostics,
    suppressions,
  };
}
