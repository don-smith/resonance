# Contributing — Requirements

Role: owns the RFC process, package authoring guide, fork guide (shared with delivery), VRS contribution conventions, and the model for external teams contributing packages back to the reference implementation.

---

## Requirements

### RFC process

- **RS.CONTRIB-R01 Significant design changes go through an RFC.** A design change is significant if it: adds a new subsystem, changes the package contract, changes the event vocabulary, changes the identity model, or changes the update delivery mechanism. Bug fixes, dependency updates, and documentation changes do not require an RFC.

- **RS.CONTRIB-R02 An RFC is a markdown file in `context/.decisions/` prefixed with its number.** Proposed RFCs have status `proposed`. Accepted RFCs have status `accepted` with a date and decider. Accepted RFCs update the relevant `requirements.md` and `spec.md` before the RFC is merged.

- **RS.CONTRIB-R03 RFCs include an evidence section.** The evidence section records what confirms the decision: prototype results, benchmark data, user feedback, or referenced prior art. "We think this is better" is not evidence.

### Package authoring

- **RS.CONTRIB-R04 The package authoring guide is maintained alongside the package contract.** When `RS.SYS.PKG` requirements or specs change, the guide is updated in the same PR.

- **RS.CONTRIB-R05 A reference package is maintained as a worked example.** The reference package demonstrates: the manifest structure, event subscribe/emit patterns, agent declaration, context provider wiring, and a Tauri command (optional Rust side). It is the primary learning resource for package authors.

### VRS maintenance

- **RS.CONTRIB-R06 ID uniqueness is enforced by CI.** See `context/spec.md` enforcement section. The CI check runs on every PR.

- **RS.CONTRIB-R07 Deltas are opened when implementation diverges from spec.** When a release ships without fully implementing a spec claim, a `.delta/` entry is opened in the same release PR. Deltas are closed when the gap is resolved.
