# Parley

Parley is a desktop speech-to-text app. Press a shortcut, speak, and paste the transcript into the app you are using.

Local models transcribe on your computer after you download them. Optional Gemini and Google Cloud Chirp 3 transcription sends audio to Google. Optional post-processing sends the transcript to your selected provider.

Parley is a fork of [Melvynx/Parler](https://github.com/Melvynx/Parler), which is based on [cjpais/Handy](https://github.com/cjpais/Handy).

## Download and install

Download Parley from the [Parley releases page](https://github.com/VXNCXNX/Parley/releases/latest). Read the [changelog](CHANGELOG.md) for release details.

For macOS, choose the DMG for your Mac:

| Mac                                   | Download architecture |
| ------------------------------------- | --------------------- |
| Apple Silicon, including M1 and later | `aarch64`             |
| Intel                                 | `x64`                 |

1. Open the DMG and drag **Parley** into **Applications**.
2. Open **Parley** from **Applications**.
3. These macOS downloads are ad-hoc signed and are not notarized. If macOS blocks the app, open **System Settings > Privacy & Security**, click **Open Anyway**, and confirm **Open**.
4. Grant **Microphone** and **Accessibility** permissions when prompted.
5. Choose a model in **Settings > Models**, download it, and configure your shortcut.

The source supports macOS, Windows, and Linux. Available installers are listed on the releases page.

## Dictation and formatting

- Use a toggle shortcut or push-to-talk to record and paste text.
- Enable live dictation to see preview text in the recording overlay with a supported native model.
- Download local models or configure Gemini or Chirp 3 with your own Google credentials.
- Choose post-processing actions manually or map them to the active app.
- Add custom words and phrases to the dictionary.
- Keep the microphone warm between recordings with a configurable timeout.
- Use a different model for longer recordings. Cloud switching requires configured credentials.

Cloud services may charge for usage. Local transcription does not require a cloud account. To keep transcripts local, also disable cloud post-processing.

## Local models

The native model catalog includes the following models:

| Model                        | Transcription mode                           |
| ---------------------------- | -------------------------------------------- |
| Nemotron Streaming 3.5       | Multilingual, with live preview              |
| Nemotron Speech Streaming EN | English, with live preview                   |
| Qwen3-ASR 0.6B               | Multilingual batch transcription             |
| Cohere Transcribe            | Multilingual batch transcription             |
| Parakeet Ultra               | Batch transcription in 25 European languages |

The catalog also includes Whisper Small, Medium, Turbo, and Large, Breeze ASR, Parakeet V2 and V3, Moonshine Base, Moonshine V2 Tiny, Small, and Medium, and SenseVoice. Supported languages appear in **Settings > Models**.

Native GGUF downloads use pinned model revisions and SHA-256 verification. Parakeet Ultra is [Moondream's refinement](https://huggingface.co/moondream/parakeet-ultra) of [NVIDIA Parakeet V3](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3). Parley uses [Nairod785's GGUF conversion](https://huggingface.co/Nairod785/parakeet-ultra-gguf/blob/b03613ba54a195238f0e915359f5a5c78269ddc6/README.md) under [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).

## Build from source

See [BUILD.md](BUILD.md) for prerequisites and platform build instructions.

For local macOS development, `bun run install:local:macos` builds, signs, installs, and launches Parley. It uses the stable `Parley Local Development` certificate so rebuilds retain the same Accessibility permission identity.

## Command-line control

Use the `parley` binary to control a running instance:

```bash
parley --toggle-transcription
parley --toggle-post-process
parley --cancel
```

Startup options include `--start-hidden`, `--no-tray`, and `--debug`. Run `parley --help` for the full list.

For a macOS app installation, call the bundled binary:

```bash
/Applications/Parley.app/Contents/MacOS/parley --toggle-transcription
```

## Troubleshooting

- For permissions or shortcut issues on macOS, check **System Settings > Privacy & Security > Microphone** and **Accessibility**.
- Open **Settings > About** to find the app data and log directories. Debug mode is available with `Cmd+Shift+D` on macOS or `Ctrl+Shift+D` on Windows and Linux.
- If automatic downloads are blocked, place model files in the `models` folder inside the app data directory. Keep the catalog filename for GGUF and Whisper files. For directory-based models, keep the catalog directory name. Restart Parley after a manual install.
- Custom Whisper GGML `.bin` models in that directory appear after restarting the app.
- On Linux, install `xdotool` for X11 or `wtype` for Wayland. `dotool` is another supported option. Wayland shortcuts can call the CLI through your desktop environment.
- Linux builds require the `gtk-layer-shell` runtime package. The recording overlay is disabled by default on Linux because some compositors let it take focus from the destination app.

Report problems in [Parley issues](https://github.com/VXNCXNX/Parley/issues), including your app version, operating system, model, and relevant logs.

## Contribute

Check [existing issues](https://github.com/VXNCXNX/Parley/issues) and [pull requests](https://github.com/VXNCXNX/Parley/pulls) before opening a contribution. Describe the problem, your change, and how you verified it on your target platform.

## License and credits

Parley uses the [MIT license](LICENSE). Model files have their own licenses.

Thanks to the authors and contributors of Handy and Parler, OpenAI Whisper, whisper.cpp and ggml, transcribe.cpp, Silero VAD, NVIDIA Parakeet, Moondream, and Tauri.
