---
name: backlog
description: Maintain Resonance Backlog Decisions safely when updating backlog/todo.yaml or its authorized plans.
---

# Maintain Backlog Decisions

Read `docs/architecture.md`, `src/packages/backlog/README.md`, `backlog/todo.yaml`, and the linked plan before changing a decision. Backlog is a repository-owned, read-only data boundary: do not add a write API, edit the package allowlist, introduce Markdown linking, or turn it into general task management.

## Edit the source safely

Keep the closed document shape:
```yaml
version: 1
decisions:
  - title: Clear decision title
    plan: plans/related-plan.md
    status: in-planning
    priority: P2
```

Use only `recently-done`, `in-progress`, `is-ready`, or `in-planning`; use only P0–P3. Priorities are relative ranks, not urgency definitions—ask the developer when rank is unclear. Preserve an existing title/plan reference unless the developer explicitly authorizes changing it. Plan references are forward-slash relative to `backlog/` and must resolve inside the repository; inspect the target before adding it.

Do not add unknown YAML keys, duplicate keys, absolute/backslash paths, or a new status/priority. Validate the real source and linked plans, then run:
```sh
bun test src/packages/backlog/backlog.test.ts
bun test
```
