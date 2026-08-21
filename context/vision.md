# Resonance — Vision

> **Note:** This `context/` tree is the founding documentation for a new Resonance application. It is temporarily staged in the current Resonance repository and should be moved to the new project repository before development begins. The existing Resonance CLI (this repo) is a separate, prior project that contributed ideas and package-contract patterns.

---

## The Problem

1. **Problem 1 — Teams lack a model for shared AI agreements that coexist with individual exploration.** AI tooling is both personal and collaborative. Individuals benefit from the freedom to experiment, define their own workflow preferences, track their own personal progress and learning journey, and document their own process at their own pace. Teams benefit from shared processes, agents, skills, assets, artifacts, documentation, and code-related agreements — and AI can help across all of those. The problem is that most teams have not figured out how to do both. There is no agreed model for how a team defines shared AI behavior, how individual members layer personal configuration on top of team agreements, or how leadership and non-developers participate in an AI-assisted team workflow. The missing piece is the handoff: what are the contracts, artifacts, and interfaces that let the individual's experimentation up-level the team as a whole, and the team's shared learning provide a richer context and learning opportunity for the individual? By serving both needs, the team benefits and the human benefits — they both benefit from each other. The goal is to up-level everyone's capability and software delivery.

2. **Problem 2 — Team tooling defaults to SaaS.** The standard answer to "how do we build team tooling" is: stand up a server, host it on managed infrastructure, add a login page, charge per seat. This creates a maintenance burden, a cost center, a privacy surface, and an infrastructure dependency — for tooling the team already runs locally on every machine.

3. **Problem 3 — Local-first and P2P are invisible to enterprise teams.** Most enterprise software teams have never seriously considered local-first or peer-to-peer architectures. These approaches are rarely demonstrated in production tooling and rarely appear in the architectural options a team considers. The absence of working examples perpetuates the absence.

4. **Problem 4 — Teams lack a shared application that serves everyone.** Planning, communication, and architectural context belong to the whole team — developers, product managers, designers, testers, and leadership. But most tooling is built for a single role: IDEs and CLIs for developers, project trackers for managers, design tools for designers. There is no cohesive application where the full team participates in the same context, each from their own vantage point. The opportunity is a team application that brings everyone together — where developers happen to be the natural starting point because they build the product, but the goal is a shared foundation that makes the whole team more effective.

5. **Problem 5 — Package extensibility requires server infrastructure.** Teams that want to extend their tooling typically need to deploy additional services, manage credentials, and maintain integrations. This raises the floor for customization above what most teams can sustain.

---

## The Vision

- **One installed binary, team-wide.** (Problems 1, 4) Every team member installs the same app — developer, product manager, designer, tester, leadership. Role determines what they see, not the binary they run.

- **Local-first, no required server.** (Problems 2, 3) The app works offline. Peers synchronize directly when online. No central server mediates content or authentication. A relay handles NAT traversal edge cases, but it carries no content authority.

- **A working demonstration of P2P for enterprise.** (Problem 3) Resonance is software teams can point to as a concrete, production-grade example that local-first and P2P are viable for the enterprise. It is not a prototype; it is a model.

- **Packages as the extensibility unit.** (Problem 5) All user-visible capability lives in packages. The runtime provides the shell, sync layer, event bus, and identity. Teams build packages in web technologies — TypeScript, any framework — without touching Rust or the runtime. They fork the app, publish updates to their team, and own the result.

- **Git remains the source of truth for code.** (Problem 2) Resonance does not add a sync layer over repository files. Git pushes and pulls are the repository's transport. Resonance reads the repo; it does not replace its version control.

- **AI capability belongs to both the team and the individual.** (Problem 1) Packages may embed AI tooling. Team-agreed AI behavior — processes, agents, skills, assets, artifacts, documentation, and code-related agreements — is checked in and shared. Individual members layer personal configuration, preferences, experimental workflows, and their own learning journey on top. The individual's discoveries up-level the team as a whole; the team's shared learning provides a richer context and learning opportunity for the individual. By serving both needs, the team benefits and the human benefits — they both benefit from each other. AI is a package concern, not a runtime concern.

- **A shared context for the whole team.** (Problem 4) Chat channels, planning documents, architectural context, and project visibility are available to every role — no Git account, terminal, or IDE required. A product manager can participate in planning and follow team conversations. A designer can review feature context alongside the team. A tester can track what's being built and share findings. Everyone operates from the same shared picture of the project.

---

## What Resonance Is Not

- Not a Git hosting service or GitHub replacement.
- Not a SaaS product. There is no hosted version and no subscription.
- Not a general-purpose chat tool designed to replace Slack or email.
- Not a task tracker. External trackers (Linear, Jira) are surfaced by packages; Resonance does not replace them.
- Not a centralized analytics platform. Visibility is peer-replicated, not server-aggregated.
- Not a security boundary between team members. The trust model assumes colleagues on the same team.

---

## Success Criteria

1. A software team with developers and non-developers installs Resonance, connects as a workspace, and uses it for planning and communication without any server infrastructure beyond a static file host for updates.
2. A developer registers their repository and sees Resonance package views (documentation, architecture, backlog) inside the workspace app without running a separate CLI or web server.
3. A package author who knows TypeScript but not Rust builds and ships a new package to their team in under a day.
4. Two team members on separate networks, behind NAT, can collaboratively edit a planning document with changes visible within two seconds.
5. A non-developer team member joins the workspace via an invite token and participates in conversations and planning without a Git account.
6. An update published by a CI pipeline reaches all team members as a notification within one hour; installation requires a single click.
7. At least one team outside the Resonance maintainers forks the app, adds a custom package, and uses it in their own workflow.
