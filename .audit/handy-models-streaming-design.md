# Selective Handy model port, design and implementation

## Problem and decision

Parley has one post-recording transcription path and keeps the full 16 kHz PCM buffer for history. Handy v0.9.7 adds GGUF inference and live partial text. A complete merge produces 66 conflicts in Parley's cloud, recording and action code. Port the needed behavior with one owner of final text.

Two read-only designs are in `/tmp/parley-upstream-investigation-20260927/architect-candidate-a.md` and `architect-candidate-b.md`. The cross-judge at `architect-judge.md` selected one final result with a private generation-keyed slot and consumer-thread feed. B's usage called finish before stopping the recorder, which would omit the resampler tail. A's one-use public handle cannot persist across Parley's global action and async stop task. The implemented slot stays inside `TranscriptionManager`; the isolated native library loads its model per session.

## Caller's usage

At start, `TranscribeAction` requests `begin_dictation` from `TranscriptionManager`, then starts recording. The manager decides whether a native session can stream. The recorder consumer feeds its processed 16 kHz frames to that dictation. At stop, `AudioRecordingManager::stop_recording` drains the queued audio and resampler tail through the same feed and returns the retained full buffer. Only then does `finish_dictation` close the feed and return the final text or a batch fallback. The existing action applies Chinese conversion, the selected or app-mapped action, history, and paste once. Cancel closes the active generation and cannot paste.

## State and ownership

`TranscriptionManager` owns one private `DictationSlot`, keyed by a generation number. The slot tracks an active worker and whether it has finished. The native worker owns its `transcribe_cpp::Session` until finalization or cancellation. Recorder frames cross a feed channel, then `Feed`, `Finalize`, and `Cancel` cross the slot channel. A drain acknowledgement on the first channel orders all audio before the finalization message. The loaded session's `supports_streaming` flag wins over catalog metadata. A second finish cannot emit or paste a second result. A stale worker result from a cancelled generation is ignored. There is no inference timeout yet.

The recorder owns sample capture and VAD. Its CoreAudio callback retains Parley's existing sample channel; a bounded ring was considered but not ported in this change. The consumer owns resampling, VAD, buffered samples, and feeding the dictation. The callback never calls inference or emits UI events. The recorder sends the Stop drain and resampler tail before replying to the action. The worker emits generation-tagged committed and tentative preview text. The overlay only displays these snapshots. It never pastes or stores tentative text.

`EngineType::TranscribeCpp` and a private `LoadedEngine::TranscribeCpp` marker select the separate native runtime. Current Whisper, Parakeet, Moonshine and SenseVoice stay on their existing engines, and Gemini and Chirp stay request/response. A four-entry catalog supplies the two Nemotron variants, Qwen3-ASR 0.6B and Cohere Transcribe 03-2026. Each entry has an immutable Hugging Face revision, expected file size and SHA-256. Existing download verification checks the hash before marking a model downloaded. Qwen and Cohere have no live partials.

For recordings above `long_audio_threshold_seconds`, finish or cancel any preview session, return its engine, and apply the existing model switch to the retained samples before choosing the final text. Partial text remains a preview of the starting model. A live dictation cannot switch models mid-audio. If the threshold does not require a switch, a successful native live final is authoritative. A failed or empty stream may batch only after the worker releases the engine. Keep cloud credential checks and model restoration on the current action path.

## Original implementation plan

1. Add `transcribe-cpp` 0.2.3 and pin `transcribe-cpp-sys` to 0.2.3. A standalone Mac probe showed that Cargo otherwise resolves sys 0.2.4 and `Model::load` rejects the version mismatch. Establish the native batch load and run before touching recording. Check `cargo check`, then a real Nemotron GGUF and 16 kHz fixture.
2. Add four model descriptors and the native load path. Verify the files and hash behavior, preserve all existing model IDs and downloads, and run Qwen and Cohere in batch when available. On Mac use Metal. Adapt Handy native library staging for Windows and Linux; verify those builds in CI.
3. Add the private dictation slot and batch compatibility for all existing models. Verify begin, cancel, repeated finish and generation transitions through behavior tests.
4. Add the recorder's bounded ring and consumer-thread feed. Prove a Stop drain and resampler tail reach both the full buffer and a live stream before finalization. Keep Parley's headset, Bluetooth and default-microphone recovery logic.
5. Add native live sessions, preview events and overlay rendering. Test partial text before Stop, final text once, cancellation, timeout and long-audio fallback. Check the existing action and app-mapping paths still paste once.
6. Compare individual Handy audio and shortcut fixes with local behavior. Port a fix only when the defect remains. Build the Mac app, run a real Nemotron dictation, check current ONNX and cloud routes, then open a ready PR.

## Accepted tradeoffs and unresolved checks

The focused catalog excludes Handy's other models until the four selected entries work. Qwen and Cohere keep the batch UX. The full PCM buffer stays in memory during live dictation because history and long-audio switching need it. A native runtime adds C++ build and packaging work. Tauri 2.9.1 hosting this runtime is not yet proven by the standalone probe; a full app build must resolve that. No model becomes the default based only on upstream scores.

The worktree baseline at `bcd303e` passes the frontend build, backend check and 51 Rust unit tests. Lint has nine pre-existing literal-string errors, and all 16 non-English locales already fail the translation consistency check. These inherited failures do not count as regressions.

## Implementation reconciliation

On 2026-09-27 a standalone probe of the exact Nemotron 3.5 Q8 file showed a native stream defect on Metal. A fresh session, 16 kHz French audio, and 560 ms frames stayed active through 6650 ms of 7648 ms, then finalized as only "Bonjour". The same file, audio, and language on CPU emitted six partial updates and the full sentence. A later CPU run with 30 ms frames and automatic language did the same. Batch transcription on Metal still returned the full sentence.

The live path for this Nemotron file therefore requests the CPU backend on macOS. Metal stays available for native batch transcription. The catalog hint does not override `supports_streaming`, and a live result is accepted only when its final text is the session result. The default selected model stays unchanged until quality is measured separately.

The app's release test loads the same file through the signed bundle library on Metal and checks that the batch text contains more than the first word. Linking both ggml engines directly in one process aborts during Metal device registration, so the native runtime lives in a separately loaded library. Release is the verified configuration for this fixture.

The app links whisper.cpp statically. Loading transcribe-cpp in the same binary makes GGUF reads enter Whisper ggml and abort. The native runtime is therefore a separate cdylib, `parley-native-runtime`, loaded at transcription time. Its own ggml symbols stay inside that library. macOS batch requests Metal there. A live request for a Nemotron filename requests CPU. Qwen and Cohere stay on batch. The app test runs the real fixture only when `PARLEY_NATIVE_FIXTURE_DIR` is set.

Linux and Windows compile the separate native runtime statically for CPU. Their packages include that shared library and do not stage dynamic ggml or Vulkan sidecar libraries. Metal batch and CPU live are verified only on this Mac.

The Mac app bundle builds and passes `codesign --verify --deep --strict`. The release library suite passes with a real Nemotron fixture. The interactive microphone-to-paste path, Linux and Windows package builds, and native inference timeout remain unverified. Existing literal-string lint, translation, and formatting checks already failed on the baseline.
