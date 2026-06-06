use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::audio;
use crate::paste;
use crate::state::AppState;

fn emit_error(app: &AppHandle, msg: &str) {
    eprintln!("ERROR: {}", msg);
    let _ = app.emit("app-error", msg.to_string());
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn register_escape(app: &AppHandle) {
    let _ = app.global_shortcut().register("Escape");
}

fn unregister_escape(app: &AppHandle) {
    let _ = app.global_shortcut().unregister("Escape");
}

fn hide_overlay(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("recorder") {
        let _ = window.hide();
    }
}

fn restore_focus(app: &AppHandle) {
    let pid = app.state::<AppState>().previous_app_pid.load(Ordering::SeqCst);
    if pid > 0 {
        paste::activate_pid(pid);
    }
}

/// Stop recording and discard audio (ESC / cancel button).
pub fn cancel_recording(app: &AppHandle) {
    let state = app.state::<AppState>();

    // If we're mid-thinking, just hide overlay (transcribe thread will see and bail).
    if !state.recorder.lock().unwrap().is_recording() {
        unregister_escape(app);
        hide_overlay(app);
        restore_focus(app);
        return;
    }

    let _ = state.recorder.lock().unwrap().stop();
    let _ = app.emit("recording-stopped", ());
    unregister_escape(app);
    hide_overlay(app);
    restore_focus(app);
}

/// Stop recording and run transcription (Alt+Space second press / confirm button).
pub fn confirm_recording(app: &AppHandle) {
    let t0 = std::time::Instant::now();
    let state = app.state::<AppState>();

    if !state.recorder.lock().unwrap().is_recording() {
        eprintln!("[confirm] called but not recording — ignored");
        return;
    }

    let result = state.recorder.lock().unwrap().stop();
    let _ = app.emit("recording-stopped", ());
    unregister_escape(app);

    match result {
        Ok((samples, sample_rate)) => {
            let duration_ms = if sample_rate > 0 {
                (samples.len() as f64 * 1000.0 / sample_rate as f64) as u64
            } else {
                0
            };
            let rms = if samples.is_empty() {
                0.0
            } else {
                (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
            };
            eprintln!(
                "[confirm] stop ok: samples={}, sr={}, dur={}ms, rms={:.6}",
                samples.len(),
                sample_rate,
                duration_ms,
                rms
            );
            if rms < 1e-6 {
                eprintln!("[confirm] aborting: rms below silence threshold");
                hide_overlay(app);
                restore_focus(app);
                let device = state.config.lock().unwrap().audio_device.clone();
                emit_error(app, &format!(
                    "No audio detected (device: \"{}\"). Check that the device is connected, or grant microphone access in System Settings > Privacy & Security > Microphone",
                    device
                ));
                return;
            }

            let samples_16k = audio::resample(&samples, sample_rate, 16000);
            eprintln!(
                "[confirm] resampled to 16kHz: {} samples ({} ms)",
                samples_16k.len(),
                samples_16k.len() / 16
            );

            let (api_key, model, language, system_prompt) = {
                let cfg = state.config.lock().unwrap();
                (
                    cfg.api_key.clone(),
                    cfg.model.clone(),
                    cfg.language.clone(),
                    cfg.system_prompt.clone(),
                )
            };

            let prompt = build_prompt(&language);

            let _ = app.emit("thinking-started", ());
            eprintln!(
                "[confirm] calling Doubao: model={}, lang={}, key_len={}, sys_prompt_len={}",
                model,
                language,
                api_key.len(),
                system_prompt.len()
            );

            let t_api = std::time::Instant::now();
            let transcription = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                state.engine.transcribe(
                    &samples_16k,
                    16000,
                    &api_key,
                    &model,
                    &prompt,
                    &system_prompt,
                )
            }));
            let api_ms = t_api.elapsed().as_millis();

            let _ = app.emit("thinking-stopped", ());
            hide_overlay(app);

            match transcription {
                Ok(Ok(text)) => {
                    eprintln!(
                        "[confirm] Doubao returned in {}ms: text_len={}, preview={:?}",
                        api_ms,
                        text.chars().count(),
                        truncate_for_log(&text, 80)
                    );
                    if text.is_empty() {
                        eprintln!("[confirm] empty result — nothing to paste (model treated as silence)");
                    } else {
                        let prev_pid = state
                            .previous_app_pid
                            .load(std::sync::atomic::Ordering::SeqCst);
                        eprintln!("[confirm] restoring focus to pid={}", prev_pid);
                        restore_focus(app);
                        std::thread::sleep(std::time::Duration::from_millis(200));
                        eprintln!("[confirm] pasting now…");
                        match paste::paste_text(&text) {
                            Ok(()) => eprintln!(
                                "[confirm] paste_text ok (total elapsed {}ms)",
                                t0.elapsed().as_millis()
                            ),
                            Err(e) => {
                                eprintln!("[confirm] paste_text FAILED: {}", e);
                                emit_error(app, &format!(
                                    "Paste failed: {}. On macOS, enable Accessibility in System Settings > Privacy & Security > Accessibility",
                                    e
                                ));
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    eprintln!("[confirm] Doubao error after {}ms: {}", api_ms, e);
                    emit_error(app, &format!("Transcription failed: {}", e));
                }
                Err(_) => {
                    eprintln!("[confirm] Doubao panicked after {}ms", api_ms);
                    emit_error(app, "Transcription crashed unexpectedly");
                }
            }
        }
        Err(e) => {
            eprintln!("[confirm] recorder.stop() failed: {}", e);
            hide_overlay(app);
            restore_focus(app);
            emit_error(app, &format!("Recording failed: {}", e));
        }
    }
}

fn truncate_for_log(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.replace('\n', "\\n")
    } else {
        let head: String = s.chars().take(max_chars).collect();
        format!("{}…(+{} chars)", head.replace('\n', "\\n"), count - max_chars)
    }
}

pub fn do_toggle_recording(app: &AppHandle) {
    let state = app.state::<AppState>();
    let is_recording = state.recorder.lock().unwrap().is_recording();

    if is_recording {
        confirm_recording(app);
    } else {
        let pid = paste::get_frontmost_pid();
        state.previous_app_pid.store(pid, Ordering::SeqCst);

        let device = state.config.lock().unwrap().audio_device.clone();

        // Show the pill immediately in a "loading" state so the user gets
        // instant visual feedback. The waveform/buttons are hidden by JS until
        // the audio device is actually capturing — `start()` blocks until then.
        //
        // NOTE: do NOT call window.set_focus() — it makes Typelessless the
        // foreground app, which then prevents us from re-focusing the user's
        // app after transcription (Windows blocks foreground-stealing from
        // non-foreground processes). The recorder window is configured with
        // `focus: false` so window.show() won't activate it either.
        if let Some(window) = app.get_webview_window("recorder") {
            position_bottom_center(app, &window);
            let _ = app.emit("recording-loading", ());
            let _ = window.show();
        }

        if let Err(e) = state.recorder.lock().unwrap().start(&device, app.clone()) {
            hide_overlay(app);
            emit_error(app, &format!("Cannot start recording: {}", e));
        } else {
            register_escape(app);
        }
    }
}

/// Place the recorder window at horizontal center, ~15% from the bottom of the
/// monitor the cursor is currently on. Falls back to the window's current
/// monitor, then the primary monitor.
fn position_bottom_center(app: &AppHandle, window: &tauri::WebviewWindow) {
    let monitor = monitor_under_cursor(app)
        .or_else(|| window.current_monitor().ok().flatten())
        .or_else(|| window.primary_monitor().ok().flatten());

    let Some(monitor) = monitor else { return };

    let screen_size = monitor.size();
    let screen_pos = monitor.position();

    let win_size = match window.outer_size() {
        Ok(s) => s,
        Err(_) => return,
    };

    // Horizontal center within the monitor.
    let x = screen_pos.x + ((screen_size.width as i32 - win_size.width as i32) / 2);

    // ~15% above the bottom edge of the monitor.
    let bottom_margin_px = (screen_size.height as f64 * 0.15).round() as i32;
    let y = screen_pos.y + screen_size.height as i32 - win_size.height as i32 - bottom_margin_px;

    let _ = window.set_position(tauri::PhysicalPosition::new(x, y));
}

/// Find the monitor whose physical bounds contain the OS cursor. Both
/// `cursor_position` and monitor bounds are in physical pixels, so they can
/// be compared directly. Returns `None` if the cursor position is unavailable
/// or no monitor matches.
fn monitor_under_cursor(app: &AppHandle) -> Option<tauri::Monitor> {
    let cursor = app.cursor_position().ok()?;
    let monitors = app.available_monitors().ok()?;

    for monitor in monitors {
        let pos = monitor.position();
        let size = monitor.size();
        let left = pos.x as f64;
        let top = pos.y as f64;
        let right = left + size.width as f64;
        let bottom = top + size.height as f64;
        if cursor.x >= left && cursor.x < right && cursor.y >= top && cursor.y < bottom {
            return Some(monitor);
        }
    }
    None
}

fn build_prompt(language: &str) -> String {
    match language {
        "en" => "Please process this audio.".to_string(),
        "fr" => "Veuillez traiter cet audio.".to_string(),
        _ => "请处理这段音频。".to_string(),
    }
}
