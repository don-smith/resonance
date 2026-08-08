---
name: explain
description: Use when a user asks what the architecture is, asks to explain a view or entity, or asks how modeled parts of Resonance relate. Produce a coherent explanation from the canonical LikeC4 model rather than an unsupported summary.
---

# Explain the architecture

Use this skill for questions such as **"explain the system context"**, **"what is X?"**, **"how does Y relate to Z?"**, or **"show me the deployment view."** The committed LikeC4 model and its named views are the source of truth for modeled architecture.

## Grounding workflow

1. Call `read_model` before making claims about the architecture. Treat its LikeC4 dump and view list as canonical; do not infer missing elements from memory.
2. Identify the requested view by its id or title. For the system context, use the matching `systemContext` or `system-context` view from the model rather than inventing a new diagram.
3. Call `read_view` for the requested view. Use its nodes and relationships as the primary outline of the explanation, preserving relationship direction and labels.
4. Call `read_entity` for important entities when their descriptions, technology, or boundaries are needed. Call `read_evidence` only for evidence paths linked by the model or view; evidence supports what exists, but does not replace modeled intent.
5. If a tool reports a recoverable parse or artifact error, explain that limitation clearly and do not present unverified details as facts.

## Response structure

For a view explanation, answer in this order:

1. **Scope** — name the view and state what boundary or question it covers.
2. **Participants** — briefly describe the important actors, systems, containers, or deployment nodes in the view.
3. **Relationships** — explain the significant flows in source-to-target order, including labels and technologies when modeled.
4. **Boundary and purpose** — explain what the view makes visible and what it deliberately leaves out. Do not confuse a view projection with the entire model.
5. **Grounding** — mention the view/entity ids or linked evidence paths that support the explanation when useful.

Prefer a short narrative followed by bullets for relationships. Use the names and descriptions from the tools. Distinguish clearly between:

- **Modeled intent** — facts in the LikeC4 model, views, and architecture metadata.
- **Implementation evidence** — repository files returned by `read_evidence`.
- **Assessment** — your own interpretation, which must be labeled as such.

Never invent a relationship, dependency, technology, user, or external system. If the canonical model does not answer part of the question, say so and identify the missing model fact. Do not claim that implementation evidence proves more than the linked file establishes.
