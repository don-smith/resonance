# Quality checklist

Run the artifact checker first. This list covers judgment and browser behavior that static checks cannot settle.

## Content and profile

- The selected profile matches the artifact's job.
- Restyling preserves every required fact, control, link, state, and script.
- The first screen states the main claim or task without a generic landing-page treatment.
- Pattern choices communicate an information need instead of filling a preferred shape.
- Adjacent section-level blocks have a visible, consistent gap. Custom block sequences use `ds-block-flow` rather than one-off outer margins.
- Experimental application patterns are labelled as experimental.

## Structure and access

- One `main` landmark and one page-level `h1` define the page.
- A skip link precedes repeated navigation.
- Heading levels do not jump.
- Native controls have visible labels and keyboard-visible focus.
- Status announcements are short. They do not replace a large live region.
- Every supported application state is reachable and recoverable where promised.
- Initial-state content marked `hidden` is actually absent from layout and the accessibility tree.

## Diagrams

- Topology matches the relationship being explained.
- Every branch and terminal outcome is drawn, not left to adjacent prose.
- Routes remain directional and do not cross labels.
- The `viewBox` encloses every node, label, route, and marker. Inspect the right and bottom edges for clipping.
- SVGs have a `viewBox`, `role="img"`, and linked `title` and `desc`.
- Color meaning has a visible legend when labels alone do not carry it.
- Wide diagrams use a labelled, keyboard-focusable scroll container at narrow widths.
- Layer stacks keep narrowing through the fourth layer and cap later insets instead of resetting a later layer to full width.

## Assets and adaptation

- The uniquely marked theme script, named palette, and canonical stylesheet match the current skill assets.
- Canonical `ds-` classes and semantic tokens carry presentation.
- No source-specific names, copied IDs, palette literals, or remote requests remain.
- New CSS expresses content-specific layout only. It does not duplicate themes or canonical components.
- `ds-ambient-field` appears only on a home, landing, or statement-led page. `ds-surface--tinted` marks a small number of focal boxes rather than every surface.
- Essential labels and summaries use `--ds-text` or `--ds-text-muted`. Reserve `--ds-text-quiet` for nonessential text, and recheck custom text inside selected rows and gradients.

## Browser review

- Check desktop and narrow widths in light and dark themes.
- Tab through links, controls, and scrollable regions.
- Confirm no console errors, failed local assets, or unexpected network calls.
- Run a browser accessibility audit. Fix every serious or critical violation, including contrast failures.
- Check computed overflow at both widths. Mark only deliberate, keyboard-scrollable regions as allowed overflow.
- Compare adjacent block bounding boxes. Document sections and `ds-block-flow` groups should keep at least `--ds-space-6` between visible children.
- Inspect ambient fields and tinted surfaces in both themes. They must fade without hard edges, obscure no text, and disappear cleanly in print.
- For document profiles, print to PDF and check page breaks, hidden chrome, diagrams, and complete secondary evidence.
