# Backlog package

## Responsibilities

The Backlog package provides the Backlog workspace. It owns the canonical `backlog/todo.yaml` projection, linked-plan rendering, and its constrained conversational agent. It is explicitly configured in `.resonance/config.json` and contributes only namespaced routes and assets.

## Configuration

```json
"backlog": {
  "module": "src/packages/backlog/index.ts",
  "provider": "openrouter",
  "model": "deepseek/deepseek-v4-flash"
}
```

The provider and model are non-secret package inputs. `openai` and the OpenAI-compatible `openrouter` provider are supported. For OpenRouter, the runtime uses `https://openrouter.ai/api/v1` and the configured model id directly. The API key is never configuration, prompt, transcript, state, response, or SSE data. It may be entered through the agent and is stored only in the repository-local, gitignored `.resonance/backlog-agent.env` with mode `0600` under the provider-specific key name (`OPENAI_API_KEY` or `OPENROUTER_API_KEY`).

## Routes and lifecycle

- `GET /api/backlog/items` returns the ordered, physically contained Decisions projection.
- `GET /api/backlog/plan?path=...` re-authorizes and renders one canonical linked plan with its metadata.
- `POST /api/backlog/metadata` deterministically updates a decision's status and/or priority.
- `GET /api/backlog/agent/state` returns non-secret conversation state.
- `GET /api/backlog/agent/events` is a snapshot-first SSE stream.
- `POST /api/backlog/agent/prompt` accepts `{ prompt, selectedPath }`.
- `POST /api/backlog/agent/credential` accepts a local key but returns no key data.
- `POST /api/backlog/agent/confirm-deletion` accepts a server-issued confirmation id.
- `POST /api/backlog/agent/reset` starts a fresh in-memory chat.

The provider runtime is lazy: navigation, selection, state reads, and SSE subscriptions do not construct the configured provider client or DeepAgents. One process-local conversation is shared across the package and concurrent prompts receive `409`.

## Authority and mutations

The agent receives a freshly re-read selected decision on every prompt. Its virtual filesystem exposes only the packaged management skill; domain tools enforce canonical paths, YAML validation, repository containment, serialized mutations, atomic individual replacements, and compensating rollback. It can review, create, edit plans, change status or priority, and request deletion. New decisions use a deterministic `plans/<kebab-case-title>.md` path derived by the domain tool; callers do not choose the plan path. Deletion always requires a visible browser confirmation and a chat request alone has no destructive effect.

Committed mutations emit a revision and affected canonical paths. The browser then re-reads the YAML and plan from disk rather than applying optimistic changes; the plan metadata controls use the same canonical store for deterministic status and priority changes. Generic filesystem, shell, arbitrary repository paths, credential inspection, network access, persistence, and cross-process transactionality are non-goals.
