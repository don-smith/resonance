# Markdown linking

Owner: Resonance

## Decision

The Docs workspace supports navigation between discovered Markdown documents through relative links.

## Scope

- Resolve relative links from the directory containing the currently displayed document.
- Intercept links only when they resolve to a discovered document using the configured Markdown extensions.
- Load matching documents into the existing Docs pane without a full page reload, preserving tree selection and the document path.
- Leave external URLs, root-relative URLs, fragments, unsupported files, query-bearing links, and links outside the discovered document set to the browser.
- Keep this behavior within Docs; do not introduce Shell-level global navigation, a URL scheme, browser history state, or cross-package dependencies.

## Completion

Relative Markdown links navigate within Docs, while non-document links retain their ordinary browser behavior. Cross-package Markdown navigation remains a future host-navigation decision.
