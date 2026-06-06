use std::sync::atomic::AtomicI32;
use std::sync::Mutex;

use crate::audio::AudioRecorder;
use crate::config::AppConfig;
use crate::stt::DoubaoEngine;

pub struct AppState {
    pub recorder: Mutex<AudioRecorder>,
    pub engine: DoubaoEngine,
    pub config: Mutex<AppConfig>,
    /// PID of the app that was focused before recording started
    pub previous_app_pid: AtomicI32,
}
