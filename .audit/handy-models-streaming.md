## Work unit: Handy 0.9.7 models and streaming

Definition of done: Parley retains its cloud transcription, actions, long-audio switching and Mac microphone behavior; installs a verified Handy catalog subset with Nemotron 3.5, Qwen3-ASR 0.6B and Cohere; builds and transcribes a fixture using Nemotron on this Mac; delivers live partials and final text through the existing overlay without duplicate paste; selected upstream audio fixes are compared and ported where needed; review-ready PR with checks and remaining platform limits.

Rigor: high. The native runtime and recording path affect all transcription. Verify the current baseline, each integration unit, a real model run, and the whole app.

### Poteto feature steps

- [x] `how` over the affected subsystem. Previous investigation traced current and Handy pipelines; rechecked entry points in this worktree.
- [x] `architect` for parallel design exploration.
- [x] Write the throughput checkpoint as four todo items.
- [x] Delegate code-writing to a subagent with exclusive worktree ownership.
- [x] Verify on the matching surface: signed Mac app bundle and real Nemotron fixture through its bundled library.
- [x] Keep the implementation in small, ordered commits. No history rewrite.
- [x] Compare the reverse migration proposed during implementation. Parley has 60 divergent commits across 180 files, including 80 files also changed by Handy; the cloud and app-action paths are absent upstream. A complete merge produces 66 conflicts. Reassess full migration as a separate project.
- [x] Open a ready PR on the isolated branch: VXNCXNX/Parley#3.

### Throughput checkpoint

- [x] Blocking first steps. Isolated worktree, baseline and architecture before code port.
- [x] Independent workstreams. Read-only design candidates in separate /tmp directories; one code owner after design.
- [x] Shared mutable state. The code owner holds this worktree exclusively until it returns; parent checks and integrates afterward.
- [x] Smallest safe decomposition. One implementation owner owns the coupled model, download, runtime and streaming flow; avoid concurrent edits to shared managers.

### Figure-it-out phases

- [x] Read poteto principles and frame the run.
- [x] Frame. Scope is one native runtime, a catalog subset, live audio flow, frontend overlay, and selected audio fixes across roughly 15-25 files. Known blockers are native build packaging and preserving Parley behavior.
- [x] Design the workflow and architecture.
- [x] Run the units, each with a check.
- [x] Keep the audit trail.
- [ ] Verify the whole and hand back.

### Architect phases

- [x] Ground
- [x] Sketch
- [x] Agree. Automatic, no user checkpoint requested.
- [x] Implement
- [ ] Scrap, only if implementation invalidates the architecture.

### Planned units

- [x] Capture baseline build and existing tests.
- [x] Integrate transcribe-cpp, packaging, and focused model catalog without regressions on Mac.
- [x] Integrate live audio session with partial/final text and cancellation.
- [x] Port the applicable resampler tail/reset fix. Existing Parley headset and shortcut recovery remains.
- [ ] Run real model transcription and app smoke, review diff, open ready PR.
