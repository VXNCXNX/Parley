## Work unit: Handy 0.9.7 models and streaming

Definition of done: Parley retains its cloud transcription, actions, long-audio switching and Mac microphone behavior; installs a verified Handy catalog subset with Nemotron 3.5, Qwen3-ASR 0.6B and Cohere; builds and transcribes a fixture using Nemotron on this Mac; delivers live partials and final text through the existing overlay without duplicate paste; selected upstream audio fixes are compared and ported where needed; review-ready PR with checks and remaining platform limits.

Rigor: high. The native runtime and recording path affect all transcription. Verify the current baseline, each integration unit, a real model run, and the whole app.

### Poteto feature steps

- [x] `how` over the affected subsystem. Previous investigation traced current and Handy pipelines; rechecked entry points in this worktree.
- [x] `architect` for parallel design exploration.
- [x] Write the throughput checkpoint as four todo items.
- [ ] Delegate code-writing to a subagent with exclusive worktree ownership.
- [ ] Verify on the matching surface.
- [ ] Rebase into small, ordered commits.
- [ ] If design is contested, `interrogate` before shipping.
- [ ] Run Opening a PR.

### Throughput checkpoint

- [x] Blocking first steps. Isolated worktree, baseline and architecture before code port.
- [x] Independent workstreams. Read-only design candidates in separate /tmp directories; one code owner after design.
- [x] Shared mutable state. The code owner holds this worktree exclusively until it returns; parent checks and integrates afterward.
- [x] Smallest safe decomposition. One implementation owner owns the coupled model, download, runtime and streaming flow; avoid concurrent edits to shared managers.

### Figure-it-out phases

- [x] Read poteto principles and frame the run.
- [x] Frame. Scope is one native runtime, a catalog subset, live audio flow, frontend overlay, and selected audio fixes across roughly 15-25 files. Known blockers are native build packaging and preserving Parley behavior.
- [x] Design the workflow and architecture.
- [ ] Run the units, each with a check.
- [ ] Keep the audit trail.
- [ ] Verify the whole and hand back.

### Architect phases

- [x] Ground
- [x] Sketch
- [x] Agree. Automatic, no user checkpoint requested.
- [ ] Implement
- [ ] Scrap, only if implementation invalidates the architecture.

### Planned units

- [x] Capture baseline build and existing tests.
- [ ] Integrate transcribe-cpp, packaging, and focused model catalog without regressions.
- [ ] Integrate live audio session with partial/final text and cancellation.
- [ ] Port applicable upstream audio and shortcut fixes after comparing local implementations.
- [ ] Run real model transcription and app smoke, review diff, open ready PR.
