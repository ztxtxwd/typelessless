mod audio;
mod commands;
mod config;
mod paste;
mod recording;
mod state;
mod stt;
mod tray;

use state::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    config::ensure_dirs();

    let cfg = config::load_config();
    let api_key_missing = cfg.api_key.trim().is_empty();

    let app_state = AppState {
        recorder: Mutex::new(audio::AudioRecorder::new()),
        engine: stt::DoubaoEngine::new(),
        config: Mutex::new(cfg),
        previous_app_pid: std::sync::atomic::AtomicI32::new(-1),
    };

    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        let app = app.clone();
                        let is_escape = shortcut.key == tauri_plugin_global_shortcut::Code::Escape;
                        std::thread::spawn(move || {
                            if is_escape {
                                recording::cancel_recording(&app);
                            } else {
                                recording::do_toggle_recording(&app);
                            }
                        });
                    }
                })
                .build(),
        )
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::list_audio_devices,
            commands::get_default_system_prompt,
            commands::change_shortcut,
            commands::cancel_recording,
            commands::confirm_recording,
        ])
        .on_window_event(|window, event| {
            if window.label() == "settings" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "macos")]
            app.handle()
                .set_activation_policy(tauri::ActivationPolicy::Accessory)?;

            paste::ensure_accessibility_permission();

            tray::setup_tray(app.handle())?;

            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let shortcut = app
                .state::<AppState>()
                .config
                .lock()
                .unwrap()
                .shortcut
                .clone();
            app.global_shortcut().register(shortcut.as_str())?;

            if api_key_missing {
                if let Some(window) = app.get_webview_window("settings") {
                    let _ = window.show();
                    let _ = window.set_focus();
                    let _ = window.center();
                }
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("Error while running Typelessless");
}
