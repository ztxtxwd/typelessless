use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

/// Thread-safe audio recorder that keeps the cpal::Stream on a dedicated thread.
pub struct AudioRecorder {
    samples: Arc<Mutex<Vec<f32>>>,
    recording: Arc<AtomicBool>,
    sample_rate: Arc<Mutex<u32>>,
    /// Handle to the recording thread (join on stop)
    thread_handle: Option<std::thread::JoinHandle<()>>,
}

// Safety: we never move cpal::Stream across threads — it lives entirely on
// the spawned thread. The fields we share are Arc-wrapped.
unsafe impl Send for AudioRecorder {}
unsafe impl Sync for AudioRecorder {}

impl AudioRecorder {
    pub fn new() -> Self {
        Self {
            samples: Arc::new(Mutex::new(Vec::new())),
            recording: Arc::new(AtomicBool::new(false)),
            sample_rate: Arc::new(Mutex::new(0)),
            thread_handle: None,
        }
    }

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }

    /// Spawn the recording thread and BLOCK until the audio stream has either
    /// successfully started capturing or failed. This way callers know that
    /// when `start()` returns Ok the microphone is actively recording.
    pub fn start(&mut self, device_name: &str, app: AppHandle) -> Result<(), String> {
        if self.is_recording() {
            return Err("Already recording".to_string());
        }

        self.samples.lock().unwrap().clear();
        self.recording.store(true, Ordering::SeqCst);

        let samples = Arc::clone(&self.samples);
        let recording = Arc::clone(&self.recording);
        let sample_rate_out = Arc::clone(&self.sample_rate);
        let device_name = device_name.to_string();

        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel::<Result<(), String>>(1);

        let handle = std::thread::spawn(move || {
            if let Err(e) = run_recording(
                device_name,
                samples,
                recording.clone(),
                sample_rate_out,
                app.clone(),
                ready_tx,
            ) {
                eprintln!("Recording error: {}", e);
                recording.store(false, Ordering::SeqCst);
                let _ = app.emit("app-error", format!("Audio error: {}", e));
            }
        });

        // Wait (with a timeout) for the audio thread to confirm capture started.
        match ready_rx.recv_timeout(std::time::Duration::from_secs(3)) {
            Ok(Ok(())) => {
                self.thread_handle = Some(handle);
                Ok(())
            }
            Ok(Err(e)) => {
                self.recording.store(false, Ordering::SeqCst);
                let _ = handle.join();
                Err(e)
            }
            Err(_) => {
                self.recording.store(false, Ordering::SeqCst);
                Err("Timed out waiting for audio device to start".to_string())
            }
        }
    }

    pub fn stop(&mut self) -> Result<(Vec<f32>, u32), String> {
        self.recording.store(false, Ordering::SeqCst);

        // Wait for the recording thread to finish
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }

        let sr = *self.sample_rate.lock().unwrap();
        let samples = std::mem::take(&mut *self.samples.lock().unwrap());

        if samples.is_empty() {
            return Err("No audio recorded".to_string());
        }

        Ok((samples, sr))
    }
}

fn run_recording(
    device_name: String,
    samples: Arc<Mutex<Vec<f32>>>,
    recording: Arc<AtomicBool>,
    sample_rate_out: Arc<Mutex<u32>>,
    app: AppHandle,
    ready_tx: std::sync::mpsc::SyncSender<Result<(), String>>,
) -> Result<(), String> {
    // Helper to forward errors back to the caller via the ready channel before
    // returning them. We only send one message on the channel — after that
    // signaling is done via the `recording` AtomicBool / app events.
    let signal_err = |tx: &std::sync::mpsc::SyncSender<Result<(), String>>, err: String| -> String {
        let _ = tx.send(Err(err.clone()));
        err
    };
    let host = cpal::default_host();

    let device = if device_name == "default" {
        match host.default_input_device() {
            Some(d) => d,
            None => return Err(signal_err(&ready_tx, "No default input device".to_string())),
        }
    } else {
        let iter = host
            .input_devices()
            .map_err(|e| signal_err(&ready_tx, format!("Cannot enumerate devices: {}", e)))?;
        match iter.into_iter().find(|d| d.name().map(|n| n == device_name).unwrap_or(false)) {
            Some(d) => d,
            None => {
                return Err(signal_err(
                    &ready_tx,
                    format!("Device '{}' not found", device_name),
                ))
            }
        }
    };

    let config = device
        .default_input_config()
        .map_err(|e| signal_err(&ready_tx, format!("No default input config: {}", e)))?;

    let sr = config.sample_rate().0;
    *sample_rate_out.lock().unwrap() = sr;
    let channels = config.channels() as usize;

    let recording_flag = Arc::clone(&recording);
    let waveform_counter = Arc::new(Mutex::new(0u32));
    let waveform_buf = Arc::new(Mutex::new(Vec::<f32>::new()));
    let wc = Arc::clone(&waveform_counter);
    let wb = Arc::clone(&waveform_buf);
    let app_clone = app.clone();

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if !recording_flag.load(Ordering::SeqCst) {
                    return;
                }

                let mono: Vec<f32> = data
                    .chunks(channels)
                    .map(|frame| frame.iter().sum::<f32>() / channels as f32)
                    .collect();

                samples.lock().unwrap().extend_from_slice(&mono);

                let mut counter = wc.lock().unwrap();
                let mut buf = wb.lock().unwrap();
                buf.extend_from_slice(&mono);
                *counter += mono.len() as u32;

                if *counter >= 800 {
                    let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32)
                        .sqrt()
                        .min(1.0);
                    let _ = app_clone.emit("waveform-update", rms);
                    buf.clear();
                    *counter = 0;
                }
            },
            |err| eprintln!("Audio stream error: {}", err),
            None,
        ),
        cpal::SampleFormat::I16 => {
            let samples = Arc::clone(&samples);
            device.build_input_stream(
                &config.into(),
                move |data: &[i16], _: &cpal::InputCallbackInfo| {
                    if !recording_flag.load(Ordering::SeqCst) {
                        return;
                    }

                    let mono: Vec<f32> = data
                        .chunks(channels)
                        .map(|frame| {
                            frame.iter().map(|&s| s as f32 / 32768.0).sum::<f32>()
                                / channels as f32
                        })
                        .collect();

                    samples.lock().unwrap().extend_from_slice(&mono);

                    let mut counter = wc.lock().unwrap();
                    let mut buf = wb.lock().unwrap();
                    buf.extend_from_slice(&mono);
                    *counter += mono.len() as u32;

                    if *counter >= 800 {
                        let rms = (buf.iter().map(|s| s * s).sum::<f32>() / buf.len() as f32)
                            .sqrt()
                            .min(1.0);
                        let _ = app_clone.emit("waveform-update", rms);
                        buf.clear();
                        *counter = 0;
                    }
                },
                |err| eprintln!("Audio stream error: {}", err),
                None,
            )
        }
        _ => {
            return Err(signal_err(
                &ready_tx,
                "Unsupported sample format".to_string(),
            ))
        }
    }
    .map_err(|e| signal_err(&ready_tx, format!("Failed to build stream: {}", e)))?;

    stream
        .play()
        .map_err(|e| signal_err(&ready_tx, format!("Failed to start stream: {}", e)))?;

    // Signal the caller that the audio device is now actively capturing.
    let _ = ready_tx.send(Ok(()));
    let _ = app.emit("recording-started", ());

    // Keep thread alive while recording
    while recording.load(Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // Stream is dropped here, stopping capture
    drop(stream);
    Ok(())
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices()
        .map(|devices| devices.filter_map(|d| d.name().ok()).collect())
        .unwrap_or_default()
}

/// Resample audio from source_rate to target_rate (linear interpolation)
pub fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
    if source_rate == target_rate {
        return samples.to_vec();
    }

    let ratio = source_rate as f64 / target_rate as f64;
    let output_len = (samples.len() as f64 / ratio) as usize;
    let mut output = Vec::with_capacity(output_len);

    for i in 0..output_len {
        let src_idx = i as f64 * ratio;
        let idx = src_idx as usize;
        let frac = src_idx - idx as f64;

        let sample = if idx + 1 < samples.len() {
            samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac
        } else if idx < samples.len() {
            samples[idx] as f64
        } else {
            0.0
        };

        output.push(sample as f32);
    }

    output
}
