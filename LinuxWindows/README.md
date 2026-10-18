# LokalBot for Linux and Windows

A native Rust port of LokalBot built with [GPUI](https://github.com/zed-industries/zed/tree/main/crates/gpui),
Zed's GPU-accelerated UI framework, and [GPUI Kit](https://github.com/longbridge/gpui-kit). It follows the
macOS app's experience (the same Today, Meetings, Timeline, People, Projects, Ask and Settings) and its
library format, so a library recorded on one platform opens on the other. Recording, transcription,
summaries, search and the read-only CLI/MCP server all run on your computer by default.

<p>
  <img src="docs/screenshots/linux-desktop-meetings.png" width="49%" alt="LokalBot on Linux">
  <img src="docs/screenshots/windows-desktop-today.png" width="49%" alt="LokalBot on Windows">
</p>

More screenshots of both apps are in [docs/screenshots](docs/screenshots).

## What works where

| Feature | Linux | Windows | Notes |
| --- | --- | --- | --- |
| Meetings: record mic + system audio | ✓ (PulseAudio/PipeWire monitor) | ✓ (WASAPI loopback) | No bot joins the call. Tracks are 16 kHz WAV files. |
| On-device transcription | ✓ | ✓ | sherpa-onnx: Qwen3-ASR 0.6B by default; Parakeet v3, SenseVoice and Whisper turbo optional. Speakers are separated with pyannote segmentation and TitaNet embeddings. |
| Notes, actions, decisions, day digest, project notes, follow-up drafts | ✓ | ✓ | The macOS prompts, run by the built-in llama.cpp server (Vulkan or CPU), Ollama, or an OpenAI-compatible server you approve. Follow-ups are written only by a model on your computer, and LokalBot never sends them. |
| Ask with citations | ✓ | ✓ | Finds the relevant meetings first (keyword and meaning search), then answers in one model turn. Every citation is checked against the sources it was given. |
| Keyword and meaning search, Quick Recall | ✓ | ✓ | SQLite FTS5 plus EmbeddingGemma vectors, kept up to date in the background. |
| Day memory (app and window activity) | X11, Hyprland, Sway, KDE; GNOME with the Window Calls extension | ✓ | Window titles are cleared after the retention window in Privacy settings. Screen text and images are not captured in this version. |
| Dictation (hold a shortcut, speak, release) | X11; Wayland through your compositor's shortcut + `lokalbot dictate` | ✓ | Transcribe inserts what you said; Compose rewrites it with the Think model. Pastes at the cursor, or copies to the clipboard when it cannot. |
| Meeting detection | ✓ (PulseAudio source outputs) | ✓ (WASAPI capture sessions) | Asks with a notification by default. |
| System tray, launch at login | ✓ (StatusNotifierItem; GNOME needs the AppIndicator extension) | ✓ | |
| `lokalbot-cli` + MCP for coding agents | ✓ | ✓ | Read-only, off until you allow it in Settings → Privacy & Data. |
| Agent Mode, autocomplete, screen text capture, live transcript | — | — | macOS only for now. |

## Install

Download the archive for your platform from the latest CI run (`lokalbot-linux-x86_64`,
`lokalbot-windows-x86_64`) or release:

- **Linux** (x86_64): extract `LokalBot-<version>-linux-x86_64.tar.gz` and run `./install.sh`. It installs
  `lokalbot` and `lokalbot-cli` into `~/.local/bin` and adds LokalBot to your applications menu.
  Runtime needs: a Vulkan or OpenGL driver (Mesa is fine), PulseAudio or PipeWire for system audio.
- **Windows** 10/11 (x64): extract `LokalBot-<version>-windows-x86_64.zip` anywhere and run `lokalbot.exe`.
  The executable is not code-signed yet, so SmartScreen may ask you to confirm.

On first launch LokalBot asks what to remember and downloads the models you choose: about 4 GB for the
default transcription, Think and search models.

Your library lives in `~/.local/share/LokalBot` on Linux and `%LOCALAPPDATA%\LokalBot` on Windows.
Set `LOKALBOT_STORAGE_ROOT` or pass `--lokalbot-storage-root <dir>` to use another folder. Logs are in
the library's `logs` folder (Settings → Advanced → Logs).

## Build from source

Requires Rust 1.99.1 (pinned in `rust-toolchain.toml`).

```bash
# Ubuntu/Debian build dependencies
sudo apt-get install -y libxkbcommon-x11-dev libwayland-dev libvulkan1 libfontconfig-dev \
  libasound2-dev libxcb1-dev libx11-xcb-dev libssl-dev pkg-config clang

cargo run -p lokalbot-app            # the app (binary: lokalbot)
cargo run -p lokalbot-cli -- list    # the CLI
cargo test --workspace
scripts/package.sh                   # release archive in dist/
```

On Windows, install Visual Studio 2022 Build Tools (C++ workload, Windows 11 SDK) and CMake, then run
the same `cargo` commands. Note that the repository's macOS test fixtures contain file names Windows
cannot create; check out only `LinuxWindows/` there (see `.github/workflows/linux-windows.yml`).

To check the whole pipeline without the UI (downloads the default models, transcribes, writes notes,
then answers a question with citations):

```bash
cargo run -p lokalbot-engine --example process_meeting -- <library> mic.wav system.wav \
  --prepare --ask "What did we decide?"
```

`cargo run -p lokalbot-app -- --seed-demo --lokalbot-storage-root <empty dir>` writes the fictional demo
library used for the screenshots.

## Layout

```text
crates/
├── lokalbot-core      Library format (macOS-compatible), SQLite search, actions, timeline, settings
├── lokalbot-ai        Model catalog, downloads, llama.cpp runtime, Think/embedding clients, prompts
├── lokalbot-audio     Microphone + system audio capture, playback, waveforms
├── lokalbot-speech    sherpa-onnx transcription, voice activity detection, speaker separation
├── lokalbot-platform  Activity sampling, global shortcuts, text insertion, tray, notifications
├── lokalbot-engine    Background work: recording, processing queue, Ask, digests, day memory, dictation
├── lokalbot-cli       Read-only CLI and stdio MCP server
└── lokalbot-app       The GPUI app
```

See [docs/dependency-provenance.md](docs/dependency-provenance.md) for how the GPUI crates were checked
against Zed's source, and [../PRIVACY.md](../PRIVACY.md) for the data and network contract, which this
port follows.
