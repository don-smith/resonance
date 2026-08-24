<!-- GENERATED FILE. DO NOT EDIT. Build from canonical design-system records. -->

# Artifact profiles

Choose one profile before composing the page.

## Application shell `application-shell`

Status: **experimental**

Small browser tools that need navigation, native controls, data density, feedback, and recoverable task states.

- Shell: application navigation, compact controls, bounded results, and task detail regions
- Density: compact application rhythm
- Width: 76rem responsive workspace
- Responsive: stack or reorder task regions without adopting document-profile rails or heroes
- Interaction: native controls, visible keyboard focus, status announcements, and reachable loading, empty, validation, and error states
- Print: not required unless the application has an explicit reporting task
- Recommended patterns: `action-cluster`, `signal-strip`, `content-card`, `form-field`, `status-banner`, `empty-state`
- Discouraged patterns: `statement-hero`, `closing-field`

Required behavior: Keep controls natively associated with labels, provide visible focus, announce status changes without replacing excessive content, preserve task order on narrow screens, and expose every supported state through working behavior.

Unsupported: The source evidence does not establish a complete application library. Authentication, settings, menus, comboboxes, tabs, pagination, destructive workflows, offline behavior, and general CRUD conventions remain unsupported.

## Rail document `rail-document`

Status: **derived**

Long technical explanations that benefit from persistent section orientation, a statement-scale opening, and wide diagrams.

- Shell: fixed top chrome with a sticky section rail and wide main content
- Density: spacious editorial rhythm with dense diagrams and evidence blocks
- Width: 76rem content field plus a 15rem rail
- Responsive: remove the rail below 60rem and preserve wide evidence in named scroll regions
- Interaction: active navigation and diagram focus may enhance a complete non-JavaScript document
- Print: remove chrome and rail, flatten shadows, and keep sections and diagrams together where practical
- Recommended patterns: `statement-hero`, `section-lead`, `verdict-callout`, `diagram-frame`, `decision-ledger`
- Discouraged patterns: none

Required behavior: Provide a skip link, fixed top chrome, one main landmark, sticky section navigation at wide widths, explicit active-navigation semantics when scripted, a readable no-JavaScript order, narrow-screen rail removal, theme support, and document print rules.

Unsupported: The profile does not establish editing, comments, multi-user review, or dense operational application workflows.

## Review packet `review-packet`

Status: **derived**

Finite documents prepared for review, approval, or structured discussion without a persistent side rail.

- Shell: centered masthead, compact jump navigation, and numbered content sections
- Density: bounded editorial packet with compact review metadata
- Width: 72rem centered page
- Responsive: reduce page gutters and keep jump navigation horizontally available
- Interaction: editing, annotation, export, and local persistence are optional adapters rather than profile requirements
- Print: omit review controls and print all decision content and secondary evidence
- Recommended patterns: `statement-hero`, `section-lead`, `decision-ledger`, `evidence-matrix`, `disclosure-group`
- Discouraged patterns: none

Required behavior: Use a bounded masthead, one main landmark, concise jump navigation, numbered section hierarchy, shared themes, a useful narrow layout, and print output that includes the full review substance.

Unsupported: The profile does not require a collaboration server, synchronized comments, durable multi-user state, or downloadable review data. Local review controls remain optional.
