---
name: html-design
description: Use when creating or restyling self-contained HTML documents, review packets, compact browser applications, or architecture diagrams.
license: MIT
---

# HTML design

Use this skill's canonical assets and utilities. Do not rebuild the design language from memory or copy subject matter from source examples.

## Required route

1. Classify the artifact as `rail-document`, `review-packet`, or `application-shell`.
2. Inventory information, relationships, behavior, and content that must survive.
3. Choose patterns by information need and diagrams by relationship type.
4. Scaffold new work. When restyling, preserve content and behavior first.
5. Adapt canonical markup and CSS. Keep the vertical rhythm and use depth sparingly.
6. Run the checker, fix every failure, then inspect desktop, narrow, light, dark, focus, and required print modes.

## 1. Classify the profile

Read [profiles.md](profiles.md) before choosing a shell. `application-shell` remains experimental.

Set `SKILL_DIR` to this file's directory. Scaffold new work with:

```bash
node "$SKILL_DIR/scripts/scaffold-artifact.mjs" \
  --profile <profile-id> \
  --palette <palette-id> \
  --output <artifact.html>
```

The scaffolder uses the default palette when `--palette` is omitted. It will not overwrite work unless `--force` is explicit.

## 2. Structure before styling

Write the argument or task order first. During restyling, preserve landmarks, controls, links, states, content, and scripts.

Documents need one page-level claim, clear sections, evidence, decisions, a next action, and a skip link before repeated navigation. Document profile sections space their direct children. Put each major pattern at that level. Wrap nested or custom block sequences in `ds-block-flow`. Never remove the gap or recreate it with one-off outer margins.

Applications need navigation, filters, results, selected detail, feedback, and reachable loading, no-results, validation, and recoverable error states. Use labelled native controls.

## 3. Select canonical patterns

Read [pattern-selection.md](pattern-selection.md), then read [examples.md](examples.md) only for selected patterns. Choose by information purpose, not shape. Do not repeat diagrams as prose cards or invent canonical variants.

## 4. Select diagram relationships

Read [diagram-selection.md](diagram-selection.md) for boundaries, containment, dependencies, sequence, branching, lifecycle, comparison, or data relationships.

Choose topology before color. Draw every terminal outcome and keep routes directional. Every SVG needs a `viewBox`, `role="img"`, and linked title and description. Put wide SVGs in a labelled, keyboard-focusable scroll region.

## 5. Use the foundations

Read [foundations.md](foundations.md). Load `assets/theme.js`, one named palette from `assets/palettes/`, and then `assets/design-system.css`. Use semantic tokens and `ds-` classes. Do not add remote assets or source-example names.

To change a scaffolded artifact without replacing its content, run:

```bash
node "$SKILL_DIR/scripts/apply-palette.mjs" \
  <artifact.html> \
  --palette <palette-id>
```

This command replaces only the marked `data-ds-palette` style block. After updating the skill, refresh all three marked assets without replacing artifact content:

```bash
node "$SKILL_DIR/scripts/refresh-artifact-assets.mjs" \
  <artifact.html> \
  --palette <palette-id>
```

Omit `--palette` to retain the artifact's named palette. Older artifacts without a named palette use the current default. The command updates or inserts the three marked blocks and converts existing local design-system asset links to inline blocks.

Depth is opt-in. Add `ds-ambient-field` to `body` on a home, landing, or statement-led page. Add `ds-surface--tinted` to one or two focal boxes. Do not tint every box or add ambient fields to routine application screens.

## 6. Check and inspect

Run from any working directory:

```bash
node "$SKILL_DIR/scripts/check-artifact.mjs" \
  <artifact.html> \
  --profile <profile-id>
```

Use `--format json` for structured output. The checker requires current, uniquely marked theme, palette, and canonical assets. Read [quality-checklist.md](quality-checklist.md), fix every failure, then inspect. Measure rendered gaps between adjacent blocks; CSS margin declarations alone prove nothing. Document profiles require print review. Narrow diagrams may scroll, but headings and controls stay outside that region.
