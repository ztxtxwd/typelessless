use tauri::AppHandle;
use tauri_plugin_global_shortcut::GlobalShortcutExt;

use crate::audio;
use crate::config;
use crate::recording;
use crate::state::AppState;
use crate::stt;

#[tauri::command]
pub fn get_config(state: tauri::State<'_, AppState>) -> config::AppConfig {
    state.config.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_config(
    config: config::AppConfig,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    config::save_config_to_disk(&config)?;
    *state.config.lock().unwrap() = config;
    Ok(())
}

#[tauri::command]
pub fn list_audio_devices() -> Vec<String> {
    audio::list_input_devices()
}

#[tauri::command]
pub fn get_default_system_prompt() -> &'static str {
    stt::DEFAULT_SYSTEM_PROMPT
}

#[tauri::command]
pub fn change_shortcut(
    shortcut: String,
    app: AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    app.global_shortcut()
        .unregister_all()
        .map_err(|e| format!("Failed to unregister shortcuts: {}", e))?;

    app.global_shortcut()
        .register(shortcut.as_str())
        .map_err(|e| format!("Invalid shortcut '{}': {}", shortcut, e))?;

    let mut cfg = state.config.lock().unwrap();
    cfg.shortcut = shortcut;
    config::save_config_to_disk(&cfg).map_err(|e| format!("Failed to save config: {}", e))?;

    Ok(())
}

#[tauri::command]
pub fn cancel_recording(app: AppHandle) {
    std::thread::spawn(move || {
        recording::cancel_recording(&app);
    });
}

#[tauri::command]
pub fn confirm_recording(app: AppHandle) {
    std::thread::spawn(move || {
        recording::confirm_recording(&app);
    });
}
