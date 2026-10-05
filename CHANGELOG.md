# Changelog

## [Unreleased]

### Fixed

- Keep Swift bridge generation available on Intel macOS hosts when cross-compiling for Apple Silicon.
- Block unsigned public releases and verify the Developer ID team, Gatekeeper assessment, and stapled notarization ticket before uploading macOS assets.
- Submit the final signed macOS DMG for notarization, staple and assess it, and verify its mounted app before upload.
- Preserve the installed macOS app's designated code requirement before replacing it, and document Accessibility permission recovery after a signing identity change.

## [0.8.10] - 2026-10-05

### Added

- Live dictation through a bundled native transcription runtime, with preview text in the recording overlay.
- Local Nemotron Streaming 3.5, Nemotron Speech Streaming EN, Qwen3-ASR 0.6B, and Cohere Transcribe models, with pinned downloads and SHA-256 verification.
- Local Parakeet Ultra support using Moondream's refinement of Parakeet V3 in GGUF format.
- Warm microphone mode with a configurable idle timeout.

### Fixed

- Complete translations for the native model catalog, cloud transcription, actions, and long recording settings in every supported locale.
- Declare the native macOS runtime as a bundled framework so Tauri signs it on both Apple Silicon and Intel.
- Skip the ARM Swift bridge when building an Intel installer on Apple Silicon.
- Keep Apple signing variables absent in ad-hoc CI builds so packaging does not try to import an empty certificate.
- Compile the Swift bridge as a library to avoid a second application entry point.
- Target macOS 11 or later so clean release builds support the native runtime's C++ filesystem dependency.
- Recover macOS transcription shortcuts after an event tap interruption.
- Keep the last working default microphone and preserve shared headset streams between recordings.
- Keep live audio and preview updates ordered, wait for the recorder's final audio, and surface native feed failures.
- Respect each native model's language support during transcription and live dictation.
- Bundle the native runtime for desktop builds and keep it loadable on macOS, including local installs.

### Changed

- Prepare macOS releases from an exact commit, with Apple Silicon and Intel downloads and optional Developer ID signing.
- Update Parley download and build instructions, and remove inherited donation links and sponsor assets.

## [0.8.1] - 2026-05-10

### Fixed

- Respect the post-processing toggle for app-to-action mappings and manual action shortcuts.
- Stabilize local macOS builds with a reusable development signing identity so Accessibility permissions survive rebuilds.

### Changed

- Added `bun run install:local:macos` for local macOS build, signing, install, and launch.
- Documented the macOS Accessibility permission and code-signing requirement for local development.

The entries below are inherited from [Handy](https://github.com/cjpais/Handy). They describe upstream history, not releases of this Parley fork.

## [0.3.0] - 2025-07-11

### Added

- **Translate to English** setting: Added automatic translation of speech to English
- Settings refactored into React hooks for better state management
- Audio device switching capability
- Hysteresis to VAD (Voice Activity Detection) for more stable recording

### Changed

- Major audio backend refactor for improved performance and reliability
- Moved audio toolkit into src-tauri directory for better permissions handling
- Model files no longer need to be downloaded separately for releases
- Updated settings components and transcription logic

### Fixed

- Audio toolkit permissions issues
- Various stability improvements

## [0.2.3] - 2025-07-03

### Fixed

- Keycode bug that was causing input issues
- Whisper model optimization: switched to unquantized Whisper Turbo, updated Whisper Medium quantization to 4_1

## [0.2.2] - 2025-07-02

### Fixed

- Removed 50ms delay feature flag for Windows (now applies to all platforms for consistency)

## [0.2.1] - 2025-07-01

### Added

- Ctrl+Space key binding for Windows platform

### Fixed

- Windows crash issue
- Model loading on startup when available
- Windows paste functionality bug

## [0.2.0] - 2025-06-30

### Added

- **Microphone activation on demand**: More efficient resource usage
- Less permissive VAD settings for better accuracy

### Changed

- Improved microphone management and activation system

## [0.1.6] - 2025-06-30

### Added

- **Multiple models support**: Users can now select from different transcription models
- Model selection onboarding flow
- Cleanup and refactoring of model management

### Changed

- Enhanced user experience with model selection interface
- Better language and UI tweaks

## [0.1.5] - 2025-06-27

### Added

- **Different start and stop recording sounds**: Enhanced audio feedback
- Recording sound samples for better user experience

## [0.1.4] - 2025-06-27

### Fixed

- Build issues
- Auto-update functionality improvements

## [0.1.3] - 2025-06-26

### Fixed

- Paste functionality using enigo library for better cross-platform compatibility

## [0.1.2] - 2025-06-26

### Added

- **Auto-update functionality**: Application can now automatically update itself
- Footer displaying current version
- Improved menu system

### Changed

- Better user interface for version management
- Enhanced update workflow

## [0.1.1] - 2025-06-25

### Added

- **Comprehensive build system**: Support for Windows, macOS, and Linux
- Windows code signing for trusted installation
- Ubuntu/Linux build support with Vulkan
- Model file download and packaging for releases
- GitHub Actions CI/CD workflow

### Changed

- Improved build process and release workflow
- Better cross-platform compatibility

### Fixed

- Various build-related issues across platforms

## [0.1.0] - 2025-05-16

### Added

- **Initial release** of Handy
- Basic speech-to-text transcription functionality
- Voice Activity Detection (VAD) for automatic recording
- Cross-platform support (macOS, Windows, Linux)
- **Tauri-based desktop application** with React frontend
- **Global keyboard shortcuts** for activation
- **Clipboard integration** for automatic text insertion
- **LLM integration** for enhanced transcription processing
- **Configurable settings** including:
  - Custom key bindings
  - Audio device selection
  - Microphone settings
  - Push-to-talk functionality
- **System tray integration** with recording indicators
- **Accessibility permissions** handling for macOS
- **Settings persistence** with unified settings store
- **Background operation** capability
- **Multiple audio format support** with on-the-fly resampling
- **Whisper model integration** for high-quality transcription
- **MIT License** for open-source distribution

### Technical Implementation

- Built with Tauri (Rust backend) and React (TypeScript frontend)
- Audio processing with cpal and whisper-rs
- Real-time transcription with performance optimizations
- Cross-platform keyboard event handling
- Modular architecture with managers for audio, models, and transcription
