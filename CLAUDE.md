# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

This project is **Typelessless**, a fork of [Light Whisper](https://github.com/aluzed/light-whisper) (MIT). The original local Whisper / Parakeet engines have been replaced by a cloud Doubao Responses-API engine.

## Build & Run

```bash
# Development (hot reload frontend, debug backend)
cargo tauri dev

# Production build
cargo tauri build

# Debug build (faster, no bundling optimization)
cargo tauri build --debug

# Build just the Rust backend (no frontend bundling)
cd src-tauri && cargo build
```

**Prerequisites:** Rust toolchain only. No CMake / C++ toolchain is needed (whisper.cpp was removed when the engine was switched to the Doubao cloud API).

**Linux note (Ubuntu 22.04+ / Debian 12+):** `sudo apt-get install -y libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libssl-dev libasound2-dev libxdo-dev build-essential`

## Architecture

Tauri v2 app with Rust backend and vanilla JS frontend (no bundler, `withGlobalTauri: true`).

### Data Flow

```
Alt+Space → recording::do_toggle_recording() → AudioRecorder.start()
  → cpal captures on dedicated thread → Arc<Mutex<Vec<f32>>>
  → emits "waveform-update" events to frontend

Alt+Space again → AudioRecorder.stop() → thread joins
  → resample to 16kHz → DoubaoEngine.transcribe()
       → POST WAV (base64 data URL) + system prompt + user prompt to ARK Responses API
  → paste::paste_text() (clipboard + Cmd/Ctrl+V simulation)
```

ESC during recording = cancel and discard. ✓ button or Alt+Space again = confirm and transcribe.

### Threading Model

- **Main thread**: Tauri windows, tray, commands
- **Shortcut handler**: spawns a new thread per Alt+Space / Esc to avoid blocking
- **Recording thread**: dedicated thread owns `cpal::Stream` (not Send — cannot cross threads)
- **Transcription**: runs synchronously on the shortcut handler thread after recording stops; the HTTP call is `reqwest::blocking`

### Shared State

`AppState` holds `recorder` (Mutex<AudioRecorder>), `engine` (DoubaoEngine, no Mutex — `reqwest::blocking::Client` is Sync), `config` (Mutex<AppConfig>), and `previous_app_pid` (AtomicI32). Mutex access uses `.lock().unwrap()`.

### Backend Modules (src-tauri/src/)

| Module | Role |
|--------|------|
| `lib.rs` | Tauri setup, plugin wiring, global shortcut handler |
| `commands.rs` | Tauri command handlers (`get_config`, `save_config`, `list_audio_devices`, `change_shortcut`, `cancel_recording`, `confirm_recording`) |
| `recording.rs` | Cancel / confirm flow, prompt building, overlay positioning |
| `audio.rs` | cpal capture on dedicated thread, linear-interpolation resample, device enumeration |
| `stt.rs` | Doubao ARK Responses API client (base64-encoded WAV → `output_text`) |
| `paste.rs` | arboard clipboard + enigo keyboard simulation, focus capture/restore |
| `config.rs` | JSON config I/O, directory paths |
| `tray.rs` | System tray menu setup |
| `state.rs` | Shared AppState |

### Frontend (src/)

Two Tauri windows, plain HTML/JS/CSS:
- **recorder** (index.html): frameless transparent overlay with waveform canvas + cancel/confirm pill buttons
- **settings** (settings.html): device, API key, model, language, shortcut

Communication: `invoke()` for commands, `event.listen()` for backend→frontend events.

### IPC Events

| Event | Direction | Payload |
|-------|-----------|---------|
| `waveform-update` | backend→frontend | `f32` RMS value |
| `recording-loading` | backend→frontend | — (overlay shown, capture not yet started) |
| `recording-started` | backend→frontend | — |
| `recording-stopped` | backend→frontend | — |
| `thinking-started` | backend→frontend | — (waiting on Doubao API) |
| `thinking-stopped` | backend→frontend | — |
| `app-error` | backend→frontend | error string |

## Key Constraints

- **cpal::Stream is not Send**: audio capture must stay on its spawning thread. `AudioRecorder` uses `unsafe impl Send/Sync` because only `Arc`-wrapped data crosses thread boundaries.
- **Doubao expects 16kHz mono PCM in a WAV container, base64-encoded as a data URL**: capture at device-native rate then linear-resample. The `input_audio` content part carries the data URL.
- **25 MB ceiling**: WAV bytes (before base64) are checked against 25 MB before sending; longer recordings need the streaming endpoint, not the Responses API.
- **Transcription is blocking**: `reqwest::blocking::Client` runs on the shortcut handler thread.
- **API-key gating**: on launch, an empty `api_key` opens the Settings window automatically.
- **Paste timing**: focus is restored to the previously-focused PID, then a 200ms sleep before keyboard simulation.

## Storage Paths

```
~/typelessless/
├── config.json              # {audio_device, language, shortcut, api_key, model}
└── temp/                    # Temporary WAV files (macOS/Linux)
```

Windows uses `%TEMP%\typelessless\` for temp files.

## Doubao System Prompt

`stt.rs` ships a long Chinese-language system prompt (`SYSTEM_PROMPT`) that tells the model to transcribe + polish + handle inline reformatting commands + emit empty string on silence. Edits to that prompt change the polishing behavior of every transcription.
