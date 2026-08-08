# Theme support: Assessment

We are much closer to theme support than it appears, and dark mode does not require a design overhaul.

## What exists already

- Styling is not inline. It lives in five package stylesheets totaling about 260 lines:
  - src/packages/shell/styles.css
  - src/packages/home/home.css
  - src/packages/docs/docs.css
  - src/packages/backlog/backlog.css
  - src/packages/architecture/architecture.css
- Shell already defines shared CSS variables such as --paper, --ink, --muted, --line, and --accent.
- First-party packages inherit and use those variables consistently. That is largely why the UI already feels coherent.
- The only thing explicitly preventing dark mode is essentially color-scheme: light plus the absence of a dark token palette and preference controller.

There are some hard-coded colors, especially in Architecture diagrams, errors, code blocks, and a few text
treatments. Those need auditing, but it is contained work.

## Recommended approach

Treat theming as a focused Shell-owned foundation, not as the completion of the entire Design System backlog item.

1.  Define semantic light and dark tokens in shell/styles.css.
2.  Add a Shell theme controller supporting:
    - Light
    - Dark
    - System
    - Browser-local persistence
    - Live reaction to operating-system changes while in System mode
3.  Apply the resolved theme to the document root and set the correct color-scheme.
4.  Add three accessible icon buttons beside the runtime version in .primary-footer.
5.  Replace remaining theme-sensitive hard-coded colors with semantic variables.
6.  Document the shared token interface so member packages can participate.

The footer is already the correct ownership seam and almost the correct layout. One caveat: it is currently hidden below 720px, so the theme control would need to remain accessible on mobile.

## Design-system relationship

The existing Design System decision already calls out:

- foundations and colors,
- CSS variables,
- themed shared components,
- lightweight and replaceable implementation,
- no framework lock-in.

Dark mode would make a very good first delivery slice of that decision. It establishes the token contract without requiring Button, Panel, Navigator, AgentChat, and all the other abstractions to be designed first.

I would avoid coupling “dark mode” to “extract every duplicated UI pattern.” That turns a tractable feature into a migration.

## Main risk

The first-party packages should adapt fairly easily. External member packages can only theme correctly if they use the documented Shell tokens; Shell cannot automatically fix package-owned hard-coded colors. That makes the token names and package-authoring guidance the important long-term interface.

Architecture/LikeC4 needs the most visual verification, although it is promising: the renderer already references --paper, and LikeC4 can observe the document color scheme.

Bottom line: this is a small-to-medium Shell feature plus a color audit, not a redesign. I would implement theme support first, use it to formalize the design-system foundations, and defer shared UI primitives until duplication justifies extracting them.
