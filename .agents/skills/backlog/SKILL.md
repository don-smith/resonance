---
name: backlog
description: Maintain Resonance Backlog Decisions safely when updating backlog/todo.yaml or its authorized plans.
---

# Maintain Backlog Decisions

Read `docs/architecture.md`, `src/packages/backlog/README.md`, `backlog/todo.yaml`, and the linked plan before changing a decision. Backlog is a repository-owned canonical data boundary with a constrained agent; do not turn it into general task management or add arbitrary filesystem access.

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

Use only `recently-done`, `in-progress`, `is-ready`, or `in-planning`; use only P0–P3. Priorities are relative ranks, not urgency definitions—ask the developer when rank is unclear. Preserve an existing title/plan reference unless explicitly authorized. Plan references are forward-slash relative to `backlog/` and must resolve inside the repository; inspect the target before adding it. Plan references must be unique.

When creating a decision, provide a title, status, priority, and Markdown only. The `create_decision` operation deterministically derives `plans/<kebab-case-title>.md`, creates the linked file inside `backlog/plans/`, and records the same path in `backlog/todo.yaml`. Do not invent or supply a plan path; the domain operation owns naming, placement, collision checks, and atomicity.

The conversational agent may perform only domain operations through its Backlog tools. It must re-read the selected canonical decision before each turn, read before writing, preserve YAML-to-plan linkage, and require explicit browser confirmation before deletion. Never expose credentials or inspect `.resonance/backlog-agent.env`; never grant generic filesystem, shell, arbitrary path, network, or durable persistence access.

Validate the real source and linked plans, then run:
```sh
bun test src/packages/backlog/backlog.test.ts
bun test
```
