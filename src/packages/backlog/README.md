# Backlog package

## Responsibilities
- Read and strictly validate `backlog/todo.yaml`.
- Serve read-only `GET /api/backlog/items` and `GET /api/backlog/plan` routes.
- Render the server-ordered Decisions projection only in Shell's supplied mount.

## Configuration
Opt in explicitly:
```json
"backlog": { "module": "src/packages/backlog/index.ts" }
```

## Data boundary
The canonical source is a closed YAML document with `version: 1` and ordered `decisions`. Every decision has non-empty `title`, forward-slash-relative `plan`, one of `recently-done`, `in-progress`, `is-ready`, `in-planning`, and P0–P3 priority. Priority is relative rank only; ask the developer when its rank is unclear. Invalid syntax/schema returns 422; missing or physically escaping source, and unauthorized plans, return 404.

Backlog does not write data, discover Markdown, or accept arbitrary paths. It resolves both YAML and every linked plan through `HostContext.resolveRepositoryPath()` (lexical and physical containment), and regenerates authorization before each plan read.
