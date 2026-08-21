# Product — Requirements

Role: owns Resonance's positioning, fit/non-fit guidance, persona definitions, and adoption model (fork, extend, publish).

---

## Assumptions

- **RS.PROD-A01 Teams are the unit of adoption.** Resonance is adopted by a team, not an individual. One member (typically a developer) forks and maintains the runtime; others receive updates automatically.

- **RS.PROD-A02 The app is not sold or subscribed.** There is no pricing, licensing tier, or usage limit. The Apache 2.0 license permits commercial use and modification. Revenue, if any, comes from consulting or support, not the software.

- **RS.PROD-A03 Promotion is by demonstration.** The app is promoted by showing it to teams — live demos, screencasts, a public fork. Word-of-mouth from working teams is the primary adoption path.

---

## Requirements

### Fit guidance

- **RS.PROD-R01 Non-fit is documented.** The product documentation explicitly states the cases Resonance is not built for: teams with adversarial internal trust, teams that require a managed SaaS service, teams with compliance requirements that prohibit local data storage. `refines: RS-A02`

- **RS.PROD-R02 Personas are the organizing frame.** All documentation, onboarding flows, and feature descriptions are organized by persona (developer, contributor, viewer). Each persona's first-run experience is explicitly designed.

### Fork and adoption model

- **RS.PROD-R03 Fork is the adoption path.** Teams adopt Resonance by forking the reference repository, not by installing from a marketplace or registering a tenant. The fork guide (see `04-contributing/`) is a first-class product deliverable.

- **RS.PROD-R04 Package authoring has a minimal floor.** A team member with TypeScript knowledge can build and ship a new package without Rust knowledge, without understanding the runtime internals, and without modifying any existing file in the runtime layer.

- **RS.PROD-R05 The reference implementation is the product.** The Resonance maintainers ship a reference fork with a curated set of default packages. Teams start from this reference. The reference fork is usable without modification for a small team.

### Demo-ability

- **RS.PROD-R06 The app can be demonstrated in under five minutes.** From install to a second peer joining a workspace and collaboratively editing a planning document takes under five minutes on a fresh machine with internet access.

- **RS.PROD-R07 The local-first story is visible.** The UI makes it clear that content is stored locally and synced P2P. Peer presence, sync status, and offline state are surfaced explicitly in the app shell.

- **RS.PROD-R08 The demo works on one machine.** A single presenter can demonstrate all features on one machine by using distinct workspace keys (tokens) for each simulated peer. This avoids requiring LAN sync or a second machine for a basic demo.

- **RS.PROD-R09 The demo scales to two machines with an invite.** A second peer can join a live demo by installing Resonance and pasting an invite key from the presenter. Both peers share the same repo, conversations, and planning documents. The demo story is simply using the product.

### Reference fork packages

- **RS.PROD-R10 Conversation is an optional onboarding package.** The reference fork includes a conversation package that is offered during onboarding (analogous to how Resonate today asks about team and personal packages at install time). The onboarding flow is the mechanism for deciding whether to include the package, not for pre-populating content.

- **RS.PROD-R11 A `#general` channel is created by default.** When the conversation package is installed, a `#general` channel is created. It starts empty — no pre-populated welcome message — but is immediately visible and postable by any user. This follows the Slack model: a single, intuitive default channel that every team member can see and use.
