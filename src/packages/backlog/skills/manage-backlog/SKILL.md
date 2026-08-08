---
name: manage-backlog
description: Use when reviewing or changing authorized Resonance Backlog decisions and their linked plans.
---

# Manage Backlog

Use the Backlog domain tools for canonical decision data and every mutation. Canonical decision paths returned by `list_decisions` are identities; do not infer paths from titles or construct file paths. The viewed repository is also mounted read-only so you can research documentation, implementation, and relationships between decisions.

1. Read the active decision context first. Use `list_decisions` and `read_plan` before working on another decision.
2. Use `ls`, `read_file`, `glob`, and `grep` when repository evidence would help answer the user or prepare a decision. Treat repository files as context, not as canonical Backlog state. Do not attempt filesystem writes.
3. Use `create_decision`, `edit_plan`, `set_status`, and `set_priority` only after the requested values are clear. Preserve unrelated decisions and summarize the tool result.
4. When creating a decision, provide only its title, status, priority, and Markdown. `create_decision` deterministically derives the linked plan path as `plans/<kebab-case-title>.md`, creates the file inside `backlog/plans/`, and records that same path in `backlog/todo.yaml`. Do not provide, invent, or alter a plan path; the tool owns naming, placement, validation, collision checks, and atomicity.
5. For deletion, call `request_delete` exactly once for the canonical path, explain that it made no change, and wait for the browser's explicit confirmation. A chat reply is not confirmation. Never claim deletion completed until the server reports it.
6. Never use shell, environment, credential, or network access. Do not inspect `.resonance/backlog-agent.env`; use Backlog domain tools rather than generic reads for `backlog/todo.yaml` and linked plans.
7. Ask for clarification rather than inventing a title, status, priority, or Markdown content.
