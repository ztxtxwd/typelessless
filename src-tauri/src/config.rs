use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub audio_device: String,
    pub language: String,
    pub shortcut: String,
    pub api_key: String,
    pub model: String,
    pub system_prompt: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            audio_device: "default".to_string(),
            language: "auto".to_string(),
            // F8 chosen over Alt+Space because Alt is uniquely painful on
            // Windows: it triggers menu-activation mode, routes injected
            // VK_PACKET as WM_SYSCHAR (which editor views drop), and lingers
            // in OS input state if the user holds the chord too long. F8 has
            // no global Windows binding, no browser binding, and although VS
            // Code uses it as "next error" in-editor, RegisterHotKey wins
            // before the editor's keymap sees the keydown.
            shortcut: "F8".to_string(),
            api_key: String::new(),
            model: "doubao-seed-2-0-lite-260428".to_string(),
            system_prompt: crate::stt::DEFAULT_SYSTEM_PROMPT.to_string(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    let home = dirs::home_dir().expect("Cannot find home directory");
    home.join("typelessless")
}

pub fn config_path() -> PathBuf {
    config_dir().join("config.json")
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    if path.exists() {
        match fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => AppConfig::default(),
        }
    } else {
        AppConfig::default()
    }
}

pub fn save_config_to_disk(config: &AppConfig) -> Result<(), String> {
    let dir = config_dir();
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create config dir: {}", e))?;

    let json =
        serde_json::to_string_pretty(config).map_err(|e| format!("Failed to serialize: {}", e))?;
    fs::write(config_path(), json).map_err(|e| format!("Failed to write config: {}", e))?;
    Ok(())
}

pub fn ensure_dirs() {
    let _ = fs::create_dir_all(config_dir());
}
