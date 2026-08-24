# Resonance — Root Requirements

Role: owns the product-level assumptions, tradeoffs, and top-level requirements that all subsystems must satisfy. Subsystem requirements refine these.

---

## Assumptions

- **RS-A01 Local machines are the primary compute.** Team members run Resonance on their own machines. The runtime does not delegate computation to a remote server.

- **RS-A02 Teams are high-trust.** The threat model is "a team member leaves the company," not "an adversarial attacker targets the workspace." Security measures are proportionate to this model.

- **RS-A03 Git manages repository content.** All content committed to a repository is managed by Git. Resonance reads this content; it does not supplement or replace Git as the sync layer for repository files.

- **RS-A04 Teams are the operators.** Teams fork the Resonance runtime, own the signing key for updates, and publish updates to their team. The Resonance maintainers provide a reference implementation, not a hosted service.

- **RS-A05 Non-developers are first-class participants.** Product managers, designers, and leadership use Resonance for conversations and planning. They do not have (or need) a Git account.

---

## Acceptable Tradeoffs

- **RS-T01 NAT traversal requires relay infrastructure.** Direct peer connections via hole-punching succeed in most network environments. Corporate proxies and aggressive firewalls require a relay fallback. The relay carries encrypted traffic but holds no content authority. This is a known constraint of P2P networking, not a design failure.

- **RS-T02 Key revocation is eventual in v1.** Removing a member from the workspace member list prevents future messages from being accepted by peers. Messages already replicated to peer stores remain. 

- **RS-T03 Bundled packages share one reviewed-code trust domain.** Tauri permissions apply to the main webview, not individual JavaScript modules. Import checks, CSP, and an explicit command allowlist prevent accidental coupling but do not sandbox bundled packages from each other. Member-loaded code requires separately labelled webviews.

- **RS-T04 Collaborative convergence requires connectivity.** Offline workspace-file edits converge on reconnection from signed operation history. Resonance merges only proven-disjoint Markdown changes; every unsafe race remains visibly resolvable rather than being silently overwritten. Teams are expected to review merged content and resolve preserved conflicts when needed.

- **RS-T05 Hole-punching requires internet access.** LAN-only operation (no internet) requires a local bootstrap peer. Automatic peer discovery without internet is a future concern.

- **RS-T06 Update delivery requires a static file host.** The auto-update manifest must be reachable from a URL. The reference implementation uses GitHub Releases and GitHub Pages. Teams self-hosting must operate an equivalent static host.

---

## Requirements

### Collaborative workspace

- **RS-R01 Local-first operation.** All content authored by the local user is readable and editable offline. Sync occurs when peers are reachable. Data loss on disconnect is not acceptable.

- **RS-R02 Peer discovery without a directory service.** Team members find each other using the workspace token. No login server, identity directory, or broker is required.

- **RS-R03 Single binary, role-determined surface.** Every team member runs the same binary. Package visibility is determined by the member's role in the workspace, not by a separate binary or installation.

- **RS-R04 Invite-based onboarding.** Any workspace member may generate an invite token. A new member joins by accepting the token through any side channel. No administrator approval step is required for the core join flow.

### Data domains

- **RS-R05 Repository content is Git-only.** Resonance never writes to a repository outside of explicit, user-initiated Git operations (e.g., saving a document edit for commit). Resonance does not add a sync layer over repository files. `refines: RS-A03`

- **RS-R06 Planning workspace files preserve offline work.** Planning content is a signed, causally ordered workspace file tree with immutable verified blobs. Members may edit a private materialized root or the rendered Markdown surface; only safe Markdown changes merge automatically, and every unsafe concurrent change remains losslessly resolvable.

- **RS-R07 Conversations are attributed and append-only.** Every message is signed by the sender's identity keypair. The conversation log is append-only in the core protocol. `refines: RS-A02`

### Extensibility

- **RS-R08 Packages are the extensibility unit.** All user-visible capability — repo views, planning surfaces, conversation UI, analytics — is implemented in packages. The runtime provides infrastructure only.

- **RS-R09 Package authors need only web skills.** A package that does not need system access (filesystem, background threads) is authored entirely in TypeScript/JavaScript. No Rust knowledge is required.

- **RS-R10 Packages communicate through declared runtime seams.** Packages do not call each other directly. Declared events carry cross-package notifications; versioned semantic capabilities provide bounded runtime operations. `refines: RS-R08`

- **RS-R11 Bundled and member packages coexist.** Reviewed bundled packages ship from the source checkout. Future member packages load from local configuration in separately labelled webviews and do not affect peers. Bundled packages win contribution conflicts. `refines: RS-R08`

### AI capability

- **RS-R12 AI tooling is a package concern.** The runtime does not prescribe an AI model, provider, or capability. Packages that embed AI tools configure them locally. Team-agreed AI behavior is version-controlled alongside the package; individual configuration layers on top.

### Conversations as a primitive

- **RS-R17 Conversations are a package-embeddable primitive.** The runtime provides the core conversation infrastructure — signed, append-only messages with CRDT sync. Packages embed conversations into any surface: planning documents, repo views, feature-specific scopes, or standalone channels. A package declares the conversations it hosts, just as it declares the events it emits and consumes. `refines: RS-R07, RS-R08, RS-R10`

### Future concerns

- **RS-F01 Agents as conversation participants.** The two package-embeddable primitives — conversations and AI tooling — can be combined: agents participate in conversations as attributed, append-only participants, invoked by `@mention` in any conversation surface (planning document comment, backlog item, channel thread). This is deferred from v1 because it requires both primitives to be stable and because agent invocation semantics (lifecycle, visibility, cost attribution) need separate design. `refines: RS-R12, RS-R17`

### Updates

- **RS-R13 Updates are automatic and verified.** The app checks for updates on launch and periodically. Available updates are presented to the user; installation requires explicit confirmation. Updates are signature-verified before installation. `refines: RS-A04`

- **RS-R14 Update delivery requires no custom server.** The update manifest is a static JSON file hosted on any reachable URL. No application server, API, or database is required for update delivery. `refines: RS-T06`

### Access control

- **RS-R15 Member removal prevents future contribution.** A member whose public key is removed from the workspace member list cannot post messages or document updates that remaining peers will accept. `refines: RS-A02, RS-T02`

- **RS-R16 Multiple repositories per workspace.** A team member may register multiple local repository clones. Repo packages load from each registered repo according to its own package manifest.
