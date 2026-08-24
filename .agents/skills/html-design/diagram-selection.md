<!-- GENERATED FILE. DO NOT EDIT. Build from canonical design-system records. -->

# Diagram selection

Choose topology from the relationship. Assign local semantic channels only after the topology and labels are clear.

| Recipe               | Supports               | Use for                         | Avoid for                                                   | Legend                  |
| -------------------- | ---------------------- | ------------------------------- | ----------------------------------------------------------- | ----------------------- |
| `branching-graph`    | branching, convergence | branching, convergence          | strict containment, simple linear sequence                  | required                |
| `comparison`         | comparison             | comparison                      | multi-step process, many comparable attributes              | when color adds meaning |
| `containment`        | containment            | containment                     | chronological flow, independent peers                       | required                |
| `data-model`         | entity-relationship    | entity-relationship             | runtime sequence, physical database layout without evidence | required                |
| `layered-dependency` | hierarchy, dependency  | hierarchy, layered dependency   | runtime sequence, unrelated categories                      | when color adds meaning |
| `lifecycle`          | sequence, branching    | state lifecycle                 | delivery phases, stateless data flow                        | required                |
| `linear-flow`        | sequence               | sequence, linear transformation | branching decisions, long process detail                    | when color adds meaning |
| `system-context`     | dependency             | dependency, system context      | internal component detail, step-by-step behavior            | required                |

Every copied SVG needs unique IDs, a `viewBox`, `role="img"`, and `aria-labelledby` references to a child title and description. Draw every branch and terminal outcome. Put wide canvases in a labelled, keyboard-focusable `ds-scroll-x` region.
