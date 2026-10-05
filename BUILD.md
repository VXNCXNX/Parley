# Build Parley

## Install prerequisites

All platforms require [Rust](https://rustup.rs/), [Bun](https://bun.sh/), CMake, and the [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your operating system.

### macOS

Install the Xcode command-line tools:

```bash
xcode-select --install
```

The Apple Silicon build uses a Swift bridge for Apple Intelligence. Enabling Apple Intelligence requires an Xcode SDK with the FoundationModels framework. Builds without that SDK use a stub. The release workflow uses macOS 26 for Apple Silicon and a native Intel runner for Intel builds.

### Windows

Install Visual Studio 2022 or its Build Tools with the C++ desktop development workload. The Windows x64 build uses CUDA for Whisper, so install a compatible NVIDIA CUDA Toolkit.

### Linux

For Ubuntu or Debian, install the native dependencies:

```bash
sudo apt update
sudo apt install build-essential libasound2-dev pkg-config libssl-dev libvulkan-dev vulkan-tools glslc libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libgtk-layer-shell0 libgtk-layer-shell-dev patchelf cmake libopenblas-dev libx11-dev libxtst-dev libxrandr-dev
```

Other distributions need equivalent development packages. Install `xdotool` for X11 or `wtype` for Wayland to paste text into other apps.

## Set up the repository

```bash
git clone https://github.com/VXNCXNX/Parley.git
cd Parley
bun install
```

The Silero VAD model is included in the repository. The build compiles and stages the pinned native transcription runtime automatically. Transcription models are downloaded from **Settings > Models** after you launch the app.

## Run the app

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
```

To run only the settings frontend, use `bun run dev`. Native audio, model inference, and system integration require the Tauri app.

## Build an installer

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build
```

Installers appear under `src-tauri/target/release/bundle/`. Builds with `--target` use `src-tauri/target/<target>/release/bundle/`.

For an ad-hoc macOS release build, use the override that disables the hardened runtime so the app can load its ad-hoc native library:

```bash
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri build \
  --target aarch64-apple-darwin --bundles app,dmg \
  --config src-tauri/tauri.release-adhoc.conf.json
```

On an Intel Mac, replace the target with `x86_64-apple-darwin`. Build each architecture on a matching Mac. The Apple Intelligence bridge is built only on Apple Silicon.

## Install a local macOS development build

```bash
bun run install:local:macos
```

This command builds Parley, signs it with the stable `Parley Local Development` identity, installs it to `/Applications/Parley.app`, and launches it. macOS ties Accessibility permission to the code-signing requirement, so a stable identity avoids a new permission identity on every rebuild.

After the first install, grant Accessibility in **System Settings > Privacy & Security > Accessibility**. Local development signing is separate from the ad-hoc release downloads.

## Prepare a GitHub release

Run the **Release** workflow on the commit you want to release. The workflow uses that exact SHA for both macOS builds and the version tag. It requires a matching version entry in `CHANGELOG.md` and reuses a draft release for that tag. An existing tag on another commit or an already published release stops the workflow.

By default, `sign-binaries` and `publish` are both false. The workflow uploads Apple Silicon and Intel DMG and app archives to a draft. Inspect the assets before publishing the draft in GitHub. Setting `publish` to true publishes only after both builds succeed and all four downloads are present.

The workflow normally uses `GITHUB_TOKEN`. To release a branch that changes workflows relative to the default branch, configure `RELEASE_TOKEN` with repository contents and workflow write permissions. GitHub requires those permissions for that release target.

To use Developer ID signing and notarization, enable `sign-binaries` and configure the Apple certificate and notarization secrets used by `.github/workflows/build.yml`. Ad-hoc releases need no Apple credentials and require **System Settings > Privacy & Security > Open Anyway** when macOS blocks the app.
