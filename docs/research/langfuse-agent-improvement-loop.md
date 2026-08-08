# Langfuse agent-improvement loop for Resonance

## Executive recommendation

Use self-hosted Langfuse v4 as the **system of record for observations, scores, annotation, experiments, prompt versions, and quality/cost dashboards**, but keep Resonance’s package-safe telemetry facade and domain safety rules authoritative. The immediate need is not more evaluators: it is a correct, complete trace shape. Then establish a small human baseline, add deterministic scores, calibrate a narrowly scoped LLM judge, and only then automate prompt/model experiments.

Langfuse v4 is generally available for self-hosting and uses an observations-first model in which LLM calls, tools, and agent steps are independently queryable; trace-level input/output is deprecated in favor of the relevant observation, usually the root span or generation ([v4 overview](https://langfuse.com/docs/v4)). Langfuse’s self-hosted OSS feature matrix includes tracing/sessions, token and cost tracking, prompt management, datasets, SDK/UI experiments, custom scores, LLM judges, human annotation/queues, dashboards, and—on v4—monitors plus the v2 Metrics/Observations APIs ([self-hosted feature matrix](https://langfuse.com/pricing-self-host)). These are available capabilities, not a ready-made Resonance improvement policy.

## What v4 provides versus what Resonance must design

| Area | Self-hosted Langfuse v4 capability | Resonance recommendation |
|---|---|---|
| Tracing | Nested observations, traces, sessions, agent/tool/generation types, OTLP ingestion | One trace per agent turn, one session per conversation; root `agent` observation with nested model attempts and tools |
| Scores and annotation | Numeric, categorical, boolean, and text scores on observations, traces, sessions, or experiment runs; score configs; UI annotation and queues ([scores](https://langfuse.com/docs/evaluation/scores/overview), [queues](https://langfuse.com/docs/evaluation/evaluation-methods/annotation-queues)) | Adopt the small score vocabulary below; do not create a single “agent quality” score |
| Evaluators | Observation/experiment LLM-as-judge and deterministic code evaluators | Deterministic checks for mechanically verifiable behavior; humans for utility; judges only for calibrated semantic dimensions |
| Code evaluators | Available when a self-hosted dispatcher is configured; `insecure-local` is trusted JS/TS only, while the production AWS Lambda dispatcher supports JS/TS and Python ([self-host setup](https://langfuse.com/self-hosting/configuration/code-evaluators)) | Initially compute repository-aware checks in Resonance’s own isolated experiment process and publish scores; do not put repository or secret access into evaluator runners |
| Datasets/experiments | Versioned dataset items with input, expected output, metadata, production-trace links, SDK/UI runs, item/run evaluators, and comparisons ([datasets](https://langfuse.com/docs/evaluation/experiments/datasets), [experiment model](https://langfuse.com/docs/evaluation/experiments/data-model)) | Build package-specific regression sets from corrected failures and run agents only against disposable fixture repositories |
| Prompt management | Immutable versions, movable labels, cached retrieval, fallback prompts, prompt-to-generation links and per-version metrics ([versioning](https://langfuse.com/docs/prompt-management/features/prompt-version-control), [trace links](https://langfuse.com/docs/prompt-management/features/link-to-traces)) | Manage behavioral prompt text there after a baseline exists; keep filesystem authority, confirmation rules, tool schemas, provider/model allowlists, and fallback prompts in code/config |
| Metrics | Dashboards over observations/scores and v2 API aggregation of cost, tokens, volume, latency, and scores; threshold monitors in v4 ([dashboards](https://langfuse.com/docs/metrics/features/custom-dashboards), [Metrics API](https://langfuse.com/docs/metrics/features/metrics-api), [monitors](https://langfuse.com/docs/metrics/features/monitors)) | Create separate quality, reliability, and efficiency views; monitor hard failures and regressions rather than a vanity composite |
| Improvement agent | APIs expose observations, scores, datasets, prompts, and experiments | Run a constrained Resonance-owned analysis job. It may publish scores and propose dataset/prompt changes, but should not promote a production prompt or relax safety policy autonomously |

Code evaluators and LLM judges do not run “for free”: self-hosted code execution needs the dispatcher above, and judge/UI prompt experiments need a configured external LLM connection. Protected prompt deployment labels are an Enterprise feature even though ordinary versions and labels are OSS ([self-hosted feature matrix](https://langfuse.com/pricing-self-host)).

## Current implementation and concrete gaps

### What already works

The host creates a repository-scoped telemetry controller, sanitizes fields, exports OTLP/HTTP JSON to `/api/public/otel/v1/traces`, sends the v4 ingestion header, maps session/model/input/output attributes, batches up to 100 records, and flushes on host disposal ([`src/telemetry.ts`](../../src/telemetry.ts), [`src/host.ts`](../../src/host.ts)). Architecture and Backlog allocate conversation session IDs, emit turn spans with user/assistant I/O, and mark outer model-stream spans as generations ([Architecture session](../../src/packages/architecture/architecture-agent.ts), [Architecture runtime](../../src/packages/architecture/architecture-deepagents.ts), [Backlog session](../../src/packages/backlog/agent-session.ts), [Backlog runtime](../../src/packages/backlog/deepagents.ts)). The sibling Pi Agent similarly records a session and turn I/O ([Pi session](../../../my-packages/src/packages/pi-agent/session.ts)). Content is redacted unless the repository enables capture, and likely secret keys/values are masked before export.

This is a useful first slice, but it is not yet an evaluable agent trace.

### Gaps, in priority order

1. **Trace semantics and parentage are wrong for v4 analysis.** `session()` allocates one trace ID that is reused by every turn span in the conversation, while the runtime’s later `child()` allocates a second trace ID that is reused by every model stream; spans have no `parentSpanId`. Backlog and Pi also clear the transcript on reset without rotating their telemetry session ID, whereas Architecture does rotate it. Langfuse recommends one self-contained unit such as one agent run per trace and a session for a multi-turn conversation ([trace guidance](https://langfuse.com/docs/observability/best-practices), [sessions](https://langfuse.com/docs/observability/features/sessions)). Events are exported as separate spans using the owning span’s ID, so an event and completed span can collide. Logs also become root-level spans. Result: two long-lived, flat, potentially invalid traces per conversation that cannot reliably target “the final turn,” infer an agent graph, or attribute a tool/model step to its parent.

2. **Exporter delivery is lossy.** A batch is removed from `pending` before `fetch`; a non-2xx response drops it, and there is no bounded retry/backoff, queue bound, or durable spool ([exporter](../../src/telemetry.ts)). Langfuse’s SDKs batch in the background and require explicit flush in short-lived processes ([data model and batching](https://langfuse.com/docs/observability/data-model)); using the supported OTel/Langfuse processor would avoid maintaining a partial exporter.

3. **Generation telemetry is incomplete.** Architecture and Backlog export model name and text but not output/total/cache/reasoning token buckets, cost, model parameters, time-to-first-token, finish reason, or prompt version. Architecture extracts only input-token context for its UI and never attaches it to the generation; Backlog extracts no usage; Pi exposes no generation at all. Langfuse tracks usage/cost only on `generation` and `embedding` observations and prioritizes provider-reported values over inference ([token/cost tracking](https://langfuse.com/docs/observability/features/token-and-cost-tracking)).

4. **Tool and retry behavior is invisible.** DeepAgents calls are streamed without a LangChain callback, package tools/backends create no `tool` observations, and Pi intentionally discards ACP tool activity. Architecture configures up to five provider retries but records only the eventual failed response, not attempt number, delay, outcome, or per-attempt cost. Langfuse defines `agent`, `tool`, and `generation` observation types, and its LangChain callback integration captures LLMs, tools, retrievers, retries, latency, and cost ([observation types](https://langfuse.com/docs/observability/features/observation-types), [LangChain integration](https://langfuse.com/integrations/frameworks/langchain)).

5. **Important dimensions are not first-class/filterable.** `repository`, `package`, `provider`, selected path/view, and request IDs are mostly raw OTel attributes. Direct OTLP maps unmapped attributes into a catch-all metadata object that is not directly filterable; filterable values need the documented Langfuse metadata prefixes, while session/environment/release/version/tags have dedicated attributes ([OTel mapping](https://langfuse.com/integrations/native/opentelemetry)). There is no environment, release/commit, prompt version, or evaluator version. Dashboards and evaluator rules will therefore be fragile.

6. **No scores or feedback path exists.** The shared contract supports logs/spans only; there is no score client, stable score ID, score config, end-user feedback control, corrected output, or annotation queue. Langfuse scores can enforce immutable configs and record their source as `API`, `EVAL`, or `ANNOTATION` ([score data model](https://langfuse.com/docs/evaluation/scores/data-model)).

7. **No offline loop exists.** There are no Langfuse datasets, experiment harnesses, candidate-vs-baseline comparisons, CI quality gates, or production-trace-to-dataset workflow. The agents are stateful and can mutate repositories, so replay must first isolate each case in a disposable fixture. Langfuse supports SDK experiments and CI regression gates, but Resonance must supply the task and oracle ([SDK experiments](https://langfuse.com/docs/evaluation/experiments/experiments-via-sdk), [CI/CD](https://langfuse.com/docs/evaluation/experiments/experiments-ci-cd)).

8. **Prompts are unversioned.** Backlog and Architecture system prompts are hardcoded, skills are package files, and generations are not linked to a prompt version ([Backlog prompt](../../src/packages/backlog/deepagents.ts), [Architecture prompt](../../src/packages/architecture/architecture-deepagents.ts)). Moving prompts before instrumentation would make changes faster but unmeasurable.

9. **Content policy is too coarse for evaluation.** The single global capture switch either hides all I/O or sends complete prompts/responses. Evaluation needs selected I/O, but repository content may contain sensitive data. Prefer an allowlist by package/observation plus deterministic client-side masking; Langfuse supports export-time masking in its JS/TS span processor ([masking](https://langfuse.com/docs/observability/features/masking)). Self-hosting changes data residency, not the need for minimization and access control.

## Score contract

Create immutable score configs with descriptions and category anchors. Attach per-turn scores to the **root agent observation**, tool checks to the **tool observation**, whole-conversation judgments to the **session**, and experiment aggregates to the **dataset run**. Use favorable polarity for quality (`true`/larger is better); keep operational quantities explicitly lower-is-better.

| Score | Type and direction | Producer | Exact meaning |
|---|---|---|---|
| `task_success` | Boolean; `true` better | **Deterministic** when an executable oracle exists; otherwise **human** | Requested end state is present and all explicit acceptance criteria hold. Do not let a judge override a deterministic failure. |
| `policy_compliance` | Boolean; `true` better; hard gate | **Deterministic** | No disallowed path/credential/network action; required confirmation occurred; writes remained in the package’s allowed boundary. |
| `artifact_valid` | Boolean; `true` better; hard gate | **Deterministic** | Post-turn LikeC4/schema/YAML/link validation succeeds for the affected package. |
| `regression_tests_pass` | Boolean; `true` better; hard gate | **Deterministic** | The case’s declared test/validation command completes successfully in the disposable fixture. |
| `tool_success` | Boolean on each tool observation; `true` better | **Deterministic** | Tool returned its declared success shape and did not throw or return a recoverable error object. |
| `retry_count` | Numeric, minimum 0; lower better; diagnostic | **Deterministic telemetry** | Number of provider/tool attempts after the first within one turn. Never treat zero retries as proof of quality. |
| `user_helpful` | Boolean; `true` better | **Human/end user** | User would accept the result without asking for a redo. Optional comment explains the failure. |
| `felt_flow` | Categorical `smooth`, `mixed`, `frustrating`; ordered from better to worse | **Human/end user**, usually on the session | The user's subjective experience of momentum and friction. This is the calibration target for automated flow proxies, not something token counts or retries can prove. |
| `flow_moment` | Boolean, positive-only; `true` means flow was explicitly reported | **Human/end user**, on the current root turn observation | The user clicked the in-product flow control at this moment. A missing score means “not reported,” never `false`; use `felt_flow` when a complete positive/negative scale is required. |
| `change_quality` | Categorical `approve=1`, `revise=0.5`, `reject=0`; higher better | **Human reviewer** | Repository changes are correct, minimal, maintainable, and appropriate—not merely valid. |
| `groundedness` | Categorical `supported=1`, `partly_supported=0.5`, `unsupported=0`; higher better | **LLM judge**, calibrated to humans | Material claims are supported by the supplied repository evidence/tool results. Use only where evidence is present in the evaluated observation. |
| `instruction_following` | Categorical `pass=1`, `minor_failure=0.5`, `major_failure=0`; higher better | **LLM judge**, calibrated to humans | Response follows explicit user requirements not already covered by deterministic policy/validity checks. |

Package-specific deterministic oracles should feed these shared dimensions: Architecture runs LikeC4 and architecture rules; Backlog validates YAML, canonical plan links, requested metadata/mutation, and deletion confirmation; Pi runs only the tests/checks declared by the case and checks requested changed paths. Latency, tokens, USD cost, tool count, and output length are **metrics**, not quality scores.

### Embedded, low-friction feedback controls

Bake feedback into the completed-response UI rather than relying only on Langfuse's annotation screen:

- `👍` / `👎` are the two values of one `user_helpful` boolean score on the root agent-turn observation. They mean “this moved me forward / I would accept it” and “this was unhelpful or took me away from the goal.” Do not add a separate check/X pair unless product research shows that response helpfulness and task completion are meaningfully different in this UI.
- `🌊` (“flow moment,” using the wave emoji) writes `flow_moment=true` on the current root turn observation. It is intentionally a one-tap positive pulse. Its absence is missing data, not evidence that the user was out of flow. If a calibrated subjective flow outcome is needed, retain the session-level `felt_flow` scale as a separate occasional/end-of-session annotation.
- Show the controls only after a turn completes, visibly mark the current selection, allow a mistaken selection to be changed or removed, and make feedback submission asynchronous so a Langfuse failure never disrupts the agent UI. An optional reason prompt after `👎` may collect a categorical friction cause, but the initial click must remain one step.

Use one narrow endpoint such as `POST /api/agent-feedback`, not one endpoint per icon. The browser should send an opaque feedback-target ID plus a constrained value; it should never receive Langfuse credentials or choose arbitrary score names. Session ID alone is insufficient for response feedback because it cannot identify which response was rated. The server must resolve the target to both the turn's trace ID and root observation ID, validate the score config, and upsert using a deterministic score ID so repeated clicks do not create duplicate rows. The wave can still be analyzed by session because the root observation already carries the session ID.

This proposal depends on the P0 trace-shape work: the telemetry API currently does not expose a stable turn/root-observation target to the browser. Generate or expose that opaque target when the turn starts, return it with the completed-response event, retain only the minimal server-side mapping needed for score publication, and test cross-session/cross-turn rejection. This is a high-value early addition once identity is correct because it produces human ground truth during normal work with almost no review burden.

### Calibration and Goodhart controls

1. Write anchored rubrics with counterexamples; label a representative, stratified set twice by two humans and adjudicate disagreements. Human-human agreement is the ceiling to understand before comparing a judge.
2. Run the judge blind on a held-out subset. For categorical/boolean scores, inspect confusion matrices, Cohen’s kappa, F1, and especially recall on severe failures; for numeric scores use rank/correlation and error. Langfuse Score Analytics supports these comparisons and coverage analysis ([Score Analytics](https://langfuse.com/docs/evaluation/scores/score-analytics)). **Recommended acceptance policy:** do not use a judge for monitoring until agreement is at least “substantial” on the local rubric and severe-failure recall is at least 90%; never use it alone as a safety or deployment gate.
3. Version judge prompt, model, rubric, and mappings together; recalibrate on any change and periodically against fresh human labels. Preserve the previous version’s scores rather than silently rewriting history.
4. Do not publish a weighted `overall_quality` score. It hides failures, rewards easy dimensions, and invites optimization against the measurement. Decide releases with a vector: all hard gates pass, utility is non-inferior to baseline, and cost/latency stay within explicit budgets. Show distributions, confidence intervals, sample size, coverage, and worst-case slices—not just means.
5. Keep rotating holdouts and adversarial cases, retain qualitative comments/corrections, review score disagreements, and never reward proxies such as verbosity, tool-call count, or absence of retries.

## Sampling policy

- **Tracing:** keep 100% while local volume is small and trace correctness is being established. Langfuse client sampling is trace-level, so dropping a trace drops all its observations and associated scores ([sampling](https://langfuse.com/docs/observability/features/sampling)). If volume later requires sampling, use a deterministic unbiased baseline and a separate mechanism to retain errors/retries; head sampling alone cannot know a later outcome.
- **Offline experiments:** evaluate every item and report per-package/per-failure-category slices. Pin the dataset version and fixture Git revision.
- **Live deterministic checks:** run on every eligible root/tool observation when cheap.
- **Live LLM judge:** begin at 5–10% of eligible root observations, using separate evaluator rules for important strata. Langfuse observation-evaluator sampling is deterministic for a given percentage ([LLM judge](https://langfuse.com/docs/evaluation/evaluation-methods/llm-as-a-judge)).
- **Human review:** each cycle annotate a stratified batch: all user-negative/policy/validity failures up to capacity, high-cost/retried and judge-uncertain cases, plus an untouched random control slice. Always publish coverage and denominators; a failure-enriched queue is not an estimate of production prevalence.

## Idempotent analysis-agent publication

Run the analysis agent out of process with read-only access to Langfuse observations/metrics and write access only to scores (and, after review, dataset candidates). For each target:

1. Select one immutable target—prefer the root turn observation—and one immutable score config.
2. Compute a stable score ID such as UUIDv5 over `project | target-kind | target-id | score-name | evaluator-version`. Do not include the value.
3. Set `name`, `configId`, target IDs, data type, value, complete comment/reasoning, and a **stable timestamp** (for example the target observation timestamp). Langfuse replaces a score only when `id`, `name`, and timestamp date all match; a matching ID alone is insufficient, and partial updates are deprecated ([idempotent score ingestion](https://langfuse.com/docs/evaluation/evaluation-methods/scores-via-sdk#preventing-duplicate-scores)).
4. Before writing, query Scores API v3 by ID; skip an identical payload, overwrite only the same evaluator version, and fail closed on target/config mismatch ([Scores API](https://langfuse.com/docs/api-and-data-platform/features/scores-api)). A new rubric/model version gets a new deterministic ID and remains comparable.
5. Publish bounded batches with retry/backoff and flush before exit. Put producer/evaluator/rubric versions and an evidence digest in score metadata/comment, not sensitive raw content. Scores posted by this external job correctly have source `API`; do not pretend they are Langfuse-managed `EVAL` scores.

## Dashboards and operating loop

After dimensions are filterable, build three dashboards segmented by repository, package, environment, release, model, and prompt version:

1. **Quality:** `task_success`, `user_helpful`, `change_quality`, groundedness/instruction-following distributions, hard-gate pass rates, judge-vs-human coverage and disagreement.
2. **Reliability:** turn/error counts, tool success by tool, retry attempts, provider status/rate limits, incomplete traces, and no-data periods.
3. **Efficiency:** p50/p95 turn and generation latency, input/output/cache/reasoning tokens, USD cost, and cost per successful turn. Spot-check provider-reported usage/cost before trusting inferred prices.

Create monitors only after a stable baseline: any `policy_compliance=false`, validity/test failures, sustained provider error rate, and statistically meaningful drops in task success or rises in p95 latency/cost. Keep alert thresholds and minimum useful sample sizes explicit; route alerts to investigation, not automatic prompt mutation.

The iterative loop is:

1. Instrument one turn as a correct trace and collect usage/tool/retry telemetry.
2. Apply deterministic scores to all eligible turns; sample human and judge scores as above.
3. Triage failures and disagreements; add corrected outputs and minimal reproducible cases to a versioned dataset.
4. Run baseline and candidate code/model/prompt versions on disposable fixtures. Compare the score vector and operational budgets; block on any hard-gate regression.
5. Human-review changed outputs and patches. Promote a candidate prompt label only after approval; canary it with release/prompt tags.
6. Monitor live slices, roll back the label/config on regression, and feed novel failures—not merely low averages—back into the dataset.

## Smallest high-value next steps

1. **P0—make traces trustworthy:** retain the telemetry facade but implement it with supported OTel/Langfuse components or equivalent semantics: fresh trace per turn, stable session per conversation, parent/child IDs, one root `agent`, real OTel events, filterable metadata/environment/release, bounded exporter retry, and forced-failure tests. Acceptance: one turn renders as one nested tree and exporter retry does not duplicate or lose the batch.
2. **P0—capture model/tool attempts:** pass a tested LangChain/DeepAgents callback or add equivalent generic callbacks; emit each actual LLM request as a generation and each tool call as a tool observation with input/output, status, attempt, model parameters, exclusive usage buckets, and provider cost. Acceptance: a forced retry and tool failure are visible once, nested correctly, with reconciled usage.
3. **P1—establish the baseline without new product UI:** create the score configs above in Langfuse, annotate 25–50 representative turns, add `felt_flow` to their containing sessions, and build the three basic dashboards. Keep content capture narrowly allowlisted and masked.
4. **P1—embed one-tap feedback:** after stable turn identity exists, add `👍`/`👎` for `user_helpful` and `🌊` for a positive `flow_moment`, backed by one constrained, idempotent server endpoint. Acceptance: feedback is attached to the intended root turn, can be changed without duplication, and cannot cross sessions or select arbitrary scores.
5. **P1—publish deterministic scores idempotently:** add a host-owned score sink or a small external job; start with policy, artifact validity, declared tests, and tool success. A rerun must not increase score row count.
6. **P2—create one regression dataset per agent:** begin with 15–30 cases each, including successes, known failures, permission/confirmation edges, malformed artifacts, provider/tool failures, and multi-turn context. Execute against temporary repository fixtures and pin dataset/fixture versions.
7. **P2—calibrate one judge:** start with `groundedness`, because Architecture already has explicit evidence. Do not add more judge dimensions until its human agreement and severe-failure recall pass the stated policy.
8. **P3—version prompts and automate experiments:** move only behavioral system prompts to Langfuse with a checked-in fallback, link prompt versions to generations, compare `candidate` against `production`, and leave promotion human-approved. Prompt retrieval must never override Resonance’s manifest or tool/write boundaries.

This order prevents Langfuse from becoming a polished dashboard over malformed traces or uncalibrated vanity scores.