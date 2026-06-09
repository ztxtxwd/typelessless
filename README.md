# Typelessless

A lightweight desktop app for global voice-to-text. Press **F8** anywhere to start recording, press again to transcribe and paste the result into the active application.

Speech is transcribed and polished by **Doubao** (Volcengine ARK Responses API), so the result you paste is already cleaned-up text — filler words removed, punctuation added, optional follow-up instructions honored.

Built with [Tauri v2](https://v2.tauri.app/) (Rust backend, vanilla JS frontend).

![demo](./demo.gif)

## Credit

Typelessless is a fork of [Light Whisper](https://github.com/aluzed/light-whisper) by Upgradists, used and redistributed under its MIT license. The original UI shell, hotkey loop, audio capture pipeline, and tray scaffolding all come from that project. The fork swaps the local Whisper / Parakeet engines for a cloud Doubao Responses-API engine and adds prompt-driven post-processing. See [LICENSE](./LICENSE) and the upstream repo for the original copyright notice.

## Features

- **Global hotkey** (F8, rebindable) works from any application
- **Doubao transcription + polishing** in one round-trip — the model fixes filler words, punctuation, and obvious disfluencies while preserving your intent
- **Inline instructions** — speaking commands like "改写成正式邮件" or "转成 bullet points" makes the model reformat the output before it gets pasted
- **Auto-paste** via clipboard + simulated Cmd/Ctrl+V into the previously focused window
- **Minimal UI** — a frameless overlay during recording, settings reachable from the tray icon
- **Auto-opens settings** on first launch when no API key is configured

## Dependencies

- [Rust](https://rustup.rs/) (1.70+)
- macOS 11+, Windows 10+, or Linux (X11 recommended)

CMake / a C++ toolchain are no longer required — the local whisper.cpp build was removed when the engine was switched to the cloud Doubao API.

### Prerequisites

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install Tauri CLI
cargo install tauri-cli --version "^2"
```

### Linux (Ubuntu 22.04+ / Debian 12+)

```bash
sudo apt-get install -y \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev \
  libssl-dev \
  libasound2-dev \
  libxdo-dev \
  build-essential
```

### Linux (Fedora 39+)

```bash
sudo dnf install -y \
  gtk3-devel \
  webkit2gtk4.1-devel \
  libayatana-appindicator-gtk3 \
  librsvg2-devel \
  openssl-devel \
  alsa-lib-devel \
  libxdo-devel \
  gcc-c++
```

## Build & Run

```bash
# Development (hot reload frontend, debug backend)
cargo tauri dev

# Production build
cargo tauri build

# Debug build (faster, no bundling optimization)
cargo tauri build --debug

# Rust backend only (no frontend bundling)
cd src-tauri && cargo build
```

## Usage

1. **Launch the app** — if no API key is configured, the Settings window opens automatically
2. **Paste your Doubao API key** (`ARK_API_KEY` from the Volcengine ARK console) and choose a model
3. **Save settings** and close the window
4. **Record** — press `F8` to start, `F8` again (or click ✓) to transcribe; press `Esc` (or click ✕) to cancel
5. **Result** — transcribed and polished text is automatically pasted into whatever app was focused
6. Access settings any time via the **tray icon**

## Configuration

| Setting | What it does |
|---------|--------------|
| Audio Input Device | cpal input device. `default` uses the OS default. |
| Doubao API Key | `ARK_API_KEY` from the [Volcengine ARK console](https://console.volcengine.com/ark). Stored in plaintext in `config.json`. |
| Doubao Model | Defaults to `doubao-seed-2-0-lite-260428`. Any ARK Responses-compatible audio-input model can be entered. |
| Language | Default prompt language sent alongside the audio (`auto`, `zh`, `en`, `fr`). |
| Shortcut | Global hotkey (default `F8`). Click **Assign** in Settings, then press the new combo. |

## Permissions (macOS)

- **Microphone**: required for audio capture. **System Settings > Privacy & Security > Microphone** → enable Typelessless. Without it macOS feeds silent audio and recording produces nothing.
- **Accessibility**: required for auto-paste (keyboard simulation). **System Settings > Privacy & Security > Accessibility** → enable Typelessless.

## Project Structure

```
typelessless/
├── src/                        # Frontend (vanilla HTML/JS/CSS)
│   ├── index.html              # Recorder overlay (frameless window)
│   ├── settings.html           # Settings (device, API key, model, language, shortcut)
│   └── settings.js / *.css
├── src-tauri/                  # Rust backend
│   ├── src/
│   │   ├── lib.rs              # Tauri setup, plugin wiring
│   │   ├── commands.rs         # Tauri command handlers
│   │   ├── recording.rs        # Recording / cancel / confirm flow
│   │   ├── audio.rs            # Audio capture (cpal) on dedicated thread
│   │   ├── stt.rs              # Doubao Responses API client
│   │   ├── paste.rs            # Clipboard + keyboard simulation (enigo)
│   │   ├── config.rs           # JSON config I/O, directory paths
│   │   ├── tray.rs             # System tray menu
│   │   └── state.rs            # Shared AppState
│   └── Cargo.toml
└── README.md
```

## Storage

```
~/typelessless/
├── config.json                # {audio_device, language, shortcut, api_key, model}
└── temp/                      # Temporary WAV files (macOS/Linux)
```

Windows uses `%TEMP%\typelessless\` for temp files.

## Privacy

Audio is sent to Volcengine's ARK API for transcription. Read their terms before using this on sensitive recordings. No audio is stored or sent anywhere else by this app.

## Community

加入用户交流群（飞书）：[applink.feishu.cn](https://applink.feishu.cn/client/chat/chatter/add_by_link?link_token=fd5n6f06-0bc5-4177-8851-3970181e7ed6)

## License

MIT — see [LICENSE](./LICENSE). Original work © Upgradists / Light Whisper contributors; modifications © Typelessless contributors.
