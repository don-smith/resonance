# Backlog evals

Owner: team

Establish an evaluation system for the backlog agent and skill to ensure correctness, reliability, and coverage.

- Define eval scenarios: create, read, update, delete decisions; status/priority transitions; error handling.
- Build automated test harness that exercises the backlog agent via the LangGraph DeepAgents SDK.
- Measure pass/fail rates and track regressions over time.
- Integrate evals into CI so changes to the agent or skill are validated before merge.
- Document how to run evals locally and interpret results.

The underlying eval system should be pluggable; the first implementation will be a local docker-hosted LangFuse server.