#[cfg(target_os = "macos")]
use crate::audio_toolkit::audio::macos_audio;
use crate::audio_toolkit::{list_input_devices, vad::SmoothedVad, AudioRecorder, SileroVad};
use crate::helpers::clamshell;
use crate::settings::{get_settings, write_settings, AppSettings};
use crate::utils;
use log::{debug, error, info, warn};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const STREAM_STALE_REFRESH: Duration = Duration::from_secs(5 * 60);
const SHARED_OUTPUT_IDLE_CLOSE_RETRY: Duration = Duration::from_secs(60);

fn set_mute(mute: bool) {
    // Expected behavior:
    // - Windows: works on most systems using standard audio drivers.
    // - Linux: works on many systems (PipeWire, PulseAudio, ALSA),
    //   but some distros may lack the tools used.
    // - macOS: works on most standard setups via AppleScript.
    // If unsupported, fails silently.

    #[cfg(target_os = "windows")]
    {
        unsafe {
            use windows::Win32::{
                Media::Audio::{
                    eMultimedia, eRender, Endpoints::IAudioEndpointVolume, IMMDeviceEnumerator,
                    MMDeviceEnumerator,
                },
                System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED},
            };

            macro_rules! unwrap_or_return {
                ($expr:expr) => {
                    match $expr {
                        Ok(val) => val,
                        Err(_) => return,
                    }
                };
            }

            // Initialize the COM library for this thread.
            // If already initialized (e.g., by another library like Tauri), this does nothing.
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            let all_devices: IMMDeviceEnumerator =
                unwrap_or_return!(CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL));
            let default_device =
                unwrap_or_return!(all_devices.GetDefaultAudioEndpoint(eRender, eMultimedia));
            let volume_interface = unwrap_or_return!(
                default_device.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
            );

            let _ = volume_interface.SetMute(mute, std::ptr::null());
        }
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;

        let mute_val = if mute { "1" } else { "0" };
        let amixer_state = if mute { "mute" } else { "unmute" };

        // Try multiple backends to increase compatibility
        // 1. PipeWire (wpctl)
        if Command::new("wpctl")
            .args(["set-mute", "@DEFAULT_AUDIO_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 2. PulseAudio (pactl)
        if Command::new("pactl")
            .args(["set-sink-mute", "@DEFAULT_SINK@", mute_val])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            return;
        }

        // 3. ALSA (amixer)
        let _ = Command::new("amixer")
            .args(["set", "Master", amixer_state])
            .output();
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let script = format!(
            "set volume output muted {}",
            if mute { "true" } else { "false" }
        );
        let _ = Command::new("osascript").args(["-e", &script]).output();
    }
}

const WHISPER_SAMPLE_RATE: usize = 16000;

struct ResolvedMicrophoneDevice {
    device: Option<cpal::Device>,
    name: Option<String>,
}

fn should_reopen_microphone_stream(current_name: Option<&str>, desired_name: Option<&str>) -> bool {
    match (current_name, desired_name) {
        (Some(current), Some(desired)) => current != desired,
        (None, Some(_)) => true,
        _ => false,
    }
}

fn should_refresh_stale_stream(opened_at: Option<Instant>, now: Instant) -> bool {
    opened_at
        .map(|opened_at| now.duration_since(opened_at) >= STREAM_STALE_REFRESH)
        .unwrap_or(false)
}

fn normalize_audio_route_name(name: &str) -> String {
    let mut normalized = name.trim().to_lowercase();
    for suffix in [" - chat", " - game", " chat", " game"] {
        if let Some(base) = normalized.strip_suffix(suffix) {
            normalized = base.trim().to_string();
            break;
        }
    }
    normalized
}

fn route_names_share_headset(input_name: &str, output_name: &str) -> bool {
    let input_base = normalize_audio_route_name(input_name);
    let output_base = normalize_audio_route_name(output_name);
    !input_base.is_empty() && input_base == output_base
}

fn should_keep_idle_stream_for_shared_output(
    input_name: Option<&str>,
    output_name: Option<&str>,
) -> bool {
    match (input_name, output_name) {
        (Some(input_name), Some(output_name)) => route_names_share_headset(input_name, output_name),
        _ => false,
    }
}

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone, Debug)]
pub enum RecordingState {
    Idle,
    Recording { binding_id: String },
}

#[derive(Clone, Debug)]
pub enum MicrophoneMode {
    AlwaysOn,
    OnDemand,
}

/* ──────────────────────────────────────────────────────────────── */

fn create_audio_recorder(
    vad_path: &str,
    app_handle: &tauri::AppHandle,
) -> Result<AudioRecorder, anyhow::Error> {
    let silero = SileroVad::new(vad_path, 0.3)
        .map_err(|e| anyhow::anyhow!("Failed to create SileroVad: {}", e))?;
    let smoothed_vad = SmoothedVad::new(Box::new(silero), 15, 15, 2);

    // Recorder with VAD plus a spectrum-level callback that forwards updates to
    // the frontend.
    let recorder = AudioRecorder::new()
        .map_err(|e| anyhow::anyhow!("Failed to create AudioRecorder: {}", e))?
        .with_vad(Box::new(smoothed_vad))
        .with_level_callback({
            let app_handle = app_handle.clone();
            move |levels| {
                utils::emit_levels(&app_handle, &levels);
            }
        });

    Ok(recorder)
}

/* ──────────────────────────────────────────────────────────────── */

#[derive(Clone)]
pub struct AudioRecordingManager {
    state: Arc<Mutex<RecordingState>>,
    mode: Arc<Mutex<MicrophoneMode>>,
    app_handle: tauri::AppHandle,

    recorder: Arc<Mutex<Option<AudioRecorder>>>,
    is_open: Arc<Mutex<bool>>,
    is_recording: Arc<Mutex<bool>>,
    did_mute: Arc<Mutex<bool>>,
    stream_opened_at: Arc<Mutex<Option<Instant>>>,
    last_working_default_microphone: Arc<Mutex<Option<String>>>,
    close_generation: Arc<AtomicU64>,
}

impl AudioRecordingManager {
    /* ---------- construction ------------------------------------------------ */

    pub fn new(app: &tauri::AppHandle) -> Result<Self, anyhow::Error> {
        let settings = get_settings(app);
        let mode = if settings.always_on_microphone {
            MicrophoneMode::AlwaysOn
        } else {
            MicrophoneMode::OnDemand
        };

        let manager = Self {
            state: Arc::new(Mutex::new(RecordingState::Idle)),
            mode: Arc::new(Mutex::new(mode.clone())),
            app_handle: app.clone(),

            recorder: Arc::new(Mutex::new(None)),
            is_open: Arc::new(Mutex::new(false)),
            is_recording: Arc::new(Mutex::new(false)),
            did_mute: Arc::new(Mutex::new(false)),
            stream_opened_at: Arc::new(Mutex::new(None)),
            last_working_default_microphone: Arc::new(Mutex::new(None)),
            close_generation: Arc::new(AtomicU64::new(0)),
        };

        // Always-on?  Open immediately.
        if matches!(mode, MicrophoneMode::AlwaysOn) {
            manager.start_microphone_stream()?;
        }

        Ok(manager)
    }

    fn schedule_lazy_close(&self, timeout: Duration) {
        let gen = self.close_generation.fetch_add(1, Ordering::SeqCst) + 1;
        let app = self.app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(timeout);
            let rm = app.state::<Arc<AudioRecordingManager>>();
            // Hold state lock across the check AND close to serialize against
            // try_start_recording, preventing a race where the stream is closed
            // under an active recording.
            let state = rm.state.lock().unwrap();
            if rm.close_generation.load(Ordering::SeqCst) == gen
                && matches!(*state, RecordingState::Idle)
            {
                if rm.should_defer_idle_close_for_shared_output() {
                    drop(state);
                    rm.schedule_lazy_close(SHARED_OUTPUT_IDLE_CLOSE_RETRY);
                    return;
                }
                info!("Closing idle microphone stream after {:?}", timeout);
                rm.stop_microphone_stream();
            }
        });
    }

    /* ---------- helper methods --------------------------------------------- */

    #[cfg(target_os = "macos")]
    fn maybe_switch_output_to_game_sibling(&self) {
        let settings = get_settings(&self.app_handle);
        let Some(input_name) = settings.selected_microphone.as_ref() else {
            return;
        };
        // Heuristic: name like "<Base> - Chat" — strip the suffix and look for
        // a sibling output named "<Base> - Game".
        let base = input_name
            .strip_suffix(" - Chat")
            .or_else(|| input_name.strip_suffix(" — Chat"))
            .or_else(|| input_name.strip_suffix(" Chat"));
        let Some(base) = base else {
            return;
        };
        let game_target = format!("{base} - Game");

        let outputs = macos_audio::list_output_device_names();
        if !outputs.iter().any(|n| n == &game_target) {
            return; // no Game sibling available, nothing to do
        }
        let current = macos_audio::get_default_output_device_name().unwrap_or_default();
        if current == game_target {
            return; // already on Game
        }
        match macos_audio::set_default_output_device_by_name(&game_target) {
            Ok(()) => {
                info!("Auto-switched system output: '{current}' -> '{game_target}' (BT Chat input freeze mitigation)");
                let _ = self.app_handle.emit(
                    "audio-output-auto-switched",
                    serde_json::json!({
                        "from": current,
                        "to": game_target,
                        "reason": "bt_chat_freeze_mitigation",
                    }),
                );
            }
            Err(e) => {
                debug!("Could not auto-switch output to '{game_target}': {e}");
            }
        }
    }

    fn current_stream_shares_current_output_route(&self) -> bool {
        let input_name = self.current_stream_device_name();

        #[cfg(target_os = "macos")]
        let output_name = macos_audio::get_default_output_device_name();

        #[cfg(not(target_os = "macos"))]
        let output_name: Option<String> = None;

        should_keep_idle_stream_for_shared_output(input_name.as_deref(), output_name.as_deref())
    }

    fn should_defer_idle_close_for_shared_output(&self) -> bool {
        let input_name = self.current_stream_device_name();

        #[cfg(target_os = "macos")]
        let output_name = macos_audio::get_default_output_device_name();

        #[cfg(not(target_os = "macos"))]
        let output_name: Option<String> = None;

        let should_defer = should_keep_idle_stream_for_shared_output(
            input_name.as_deref(),
            output_name.as_deref(),
        );

        if should_defer {
            info!(
                "Keeping idle microphone stream open because input '{}' shares the current output '{}'; closing it would trigger a headset audio route change",
                input_name.as_deref().unwrap_or("unknown"),
                output_name.as_deref().unwrap_or("unknown")
            );
        }

        should_defer
    }

    fn configured_microphone_name<'a>(&self, settings: &'a AppSettings) -> Option<&'a String> {
        // Check if we're in clamshell mode and have a clamshell microphone configured
        let use_clamshell_mic = if let Ok(is_clamshell) = clamshell::is_clamshell() {
            is_clamshell && settings.clamshell_microphone.is_some()
        } else {
            false
        };

        if use_clamshell_mic {
            settings.clamshell_microphone.as_ref()
        } else {
            settings.selected_microphone.as_ref()
        }
    }

    fn resolve_effective_microphone_device(
        &self,
        settings: &AppSettings,
    ) -> ResolvedMicrophoneDevice {
        let configured_name = self.configured_microphone_name(settings).cloned();

        match list_input_devices() {
            Ok(devices) => {
                if let Some(device_name) = configured_name.as_ref() {
                    if let Some(device) = devices.iter().find(|d| d.name == *device_name) {
                        return ResolvedMicrophoneDevice {
                            device: Some(device.device.clone()),
                            name: Some(device.name.clone()),
                        };
                    }

                    warn!(
                        "Configured microphone '{}' was not found; falling back to system default",
                        device_name
                    );
                }

                if configured_name.is_none() {
                    let sticky_name = self
                        .last_working_default_microphone
                        .lock()
                        .unwrap()
                        .clone()
                        .or_else(|| settings.last_working_default_microphone.clone());

                    if let Some(sticky_name) = sticky_name {
                        if let Some(device) = devices.iter().find(|d| d.name == sticky_name) {
                            debug!("Using last working default microphone: {sticky_name}");
                            return ResolvedMicrophoneDevice {
                                device: Some(device.device.clone()),
                                name: Some(device.name.clone()),
                            };
                        }
                    }
                }

                if let Some(device) = devices.into_iter().find(|d| d.is_default) {
                    return ResolvedMicrophoneDevice {
                        name: Some(device.name.clone()),
                        device: Some(device.device),
                    };
                }

                ResolvedMicrophoneDevice {
                    device: None,
                    name: None,
                }
            }
            Err(e) => {
                debug!("Failed to list devices, using default: {}", e);
                ResolvedMicrophoneDevice {
                    device: None,
                    name: None,
                }
            }
        }
    }

    fn current_stream_device_name(&self) -> Option<String> {
        self.recorder
            .lock()
            .unwrap()
            .as_ref()
            .and_then(|recorder| recorder.device_name())
    }

    fn remember_working_default_microphone(&self, settings: &AppSettings, sample_count: usize) {
        if sample_count == 0 || self.configured_microphone_name(settings).is_some() {
            return;
        }

        if let Some(device_name) = self.current_stream_device_name() {
            *self.last_working_default_microphone.lock().unwrap() = Some(device_name.clone());
            let mut updated_settings = settings.clone();
            updated_settings.last_working_default_microphone = Some(device_name.clone());
            write_settings(&self.app_handle, updated_settings);
            debug!("Remembered working default microphone: {device_name}");
        }
    }

    fn ensure_microphone_stream_ready(&self) -> Result<(), anyhow::Error> {
        if !*self.is_open.lock().unwrap() {
            self.start_microphone_stream()?;
            return Ok(());
        }

        let settings = get_settings(&self.app_handle);
        let desired_device = self.resolve_effective_microphone_device(&settings);
        let current_device_name = self.current_stream_device_name();
        let stale_stream =
            should_refresh_stale_stream(*self.stream_opened_at.lock().unwrap(), Instant::now())
                && !self.current_stream_shares_current_output_route();

        if should_reopen_microphone_stream(
            current_device_name.as_deref(),
            desired_device.name.as_deref(),
        ) || stale_stream
        {
            let reason = if stale_stream {
                "stream is stale"
            } else {
                "effective device changed"
            };
            info!(
                "Refreshing microphone stream before recording ({reason}): '{}' -> '{}'",
                current_device_name.as_deref().unwrap_or("unknown"),
                desired_device.name.as_deref().unwrap_or("unknown")
            );
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream();
            self.start_microphone_stream()?;
        }

        Ok(())
    }

    /* ---------- microphone life-cycle -------------------------------------- */

    /// Applies mute if mute_while_recording is enabled and stream is open
    pub fn apply_mute(&self) {
        let settings = get_settings(&self.app_handle);
        let mut did_mute_guard = self.did_mute.lock().unwrap();

        if settings.mute_while_recording && *self.is_open.lock().unwrap() {
            set_mute(true);
            *did_mute_guard = true;
            debug!("Mute applied");
        }
    }

    /// Removes mute if it was applied
    pub fn remove_mute(&self) {
        let mut did_mute_guard = self.did_mute.lock().unwrap();
        if *did_mute_guard {
            set_mute(false);
            *did_mute_guard = false;
            debug!("Mute removed");
        }
    }

    pub fn start_microphone_stream(&self) -> Result<(), anyhow::Error> {
        let mut open_flag = self.is_open.lock().unwrap();
        if *open_flag {
            debug!("Microphone stream already active");
            return Ok(());
        }

        // macOS: if the selected mic is a Bluetooth/USB headset whose name ends
        // in "Chat" (low-quality SCO/HFP profile), proactively switch the system
        // output to the matching "Game" sibling. This avoids forcing the headset
        // into bidirectional SCO mode, which on some devices triggers a
        // CoreAudio hang that can freeze the entire OS (see panic dump
        // 2026-05-08 / forceReset btn_rst).
        #[cfg(target_os = "macos")]
        self.maybe_switch_output_to_game_sibling();

        let start_time = Instant::now();

        // Don't mute immediately - caller will handle muting after audio feedback
        let mut did_mute_guard = self.did_mute.lock().unwrap();
        *did_mute_guard = false;

        let vad_path = self
            .app_handle
            .path()
            .resolve(
                "resources/models/silero_vad_v4.onnx",
                tauri::path::BaseDirectory::Resource,
            )
            .map_err(|e| anyhow::anyhow!("Failed to resolve VAD path: {}", e))?;
        let mut recorder_opt = self.recorder.lock().unwrap();

        if recorder_opt.is_none() {
            *recorder_opt = Some(create_audio_recorder(
                vad_path.to_str().unwrap(),
                &self.app_handle,
            )?);
        }

        // Get the selected device from settings, considering clamshell mode
        let settings = get_settings(&self.app_handle);
        let selected_device = self.resolve_effective_microphone_device(&settings);

        // Pre-flight: if no device is selected/configured AND no devices exist,
        // surface a clear error instead of a cryptic backend message.
        if selected_device.device.is_none() {
            let has_any_device = list_input_devices()
                .map(|devices| !devices.is_empty())
                .unwrap_or(false);
            if !has_any_device {
                return Err(anyhow::anyhow!("No input device found"));
            }
        }

        if let Some(rec) = recorder_opt.as_mut() {
            rec.open(selected_device.device)
                .map_err(|e| anyhow::anyhow!("Failed to open recorder: {}", e))?;
        }

        *open_flag = true;
        *self.stream_opened_at.lock().unwrap() = Some(Instant::now());
        info!(
            "Microphone stream initialized in {:?}",
            start_time.elapsed()
        );
        Ok(())
    }

    pub fn stop_microphone_stream(&self) {
        let mut open_flag = self.is_open.lock().unwrap();
        if !*open_flag {
            return;
        }

        let mut did_mute_guard = self.did_mute.lock().unwrap();
        if *did_mute_guard {
            set_mute(false);
        }
        *did_mute_guard = false;

        if let Some(rec) = self.recorder.lock().unwrap().as_mut() {
            // If still recording, stop first.
            if *self.is_recording.lock().unwrap() {
                let _ = rec.stop();
                *self.is_recording.lock().unwrap() = false;
            }
            let _ = rec.close();
        }

        *open_flag = false;
        *self.stream_opened_at.lock().unwrap() = None;
        debug!("Microphone stream stopped");
    }

    /* ---------- mode switching --------------------------------------------- */

    pub fn update_mode(&self, new_mode: MicrophoneMode) -> Result<(), anyhow::Error> {
        let cur_mode = self.mode.lock().unwrap().clone();

        match (cur_mode, &new_mode) {
            (MicrophoneMode::AlwaysOn, MicrophoneMode::OnDemand) => {
                if matches!(*self.state.lock().unwrap(), RecordingState::Idle) {
                    self.close_generation.fetch_add(1, Ordering::SeqCst);
                    if self.should_defer_idle_close_for_shared_output() {
                        self.schedule_lazy_close(SHARED_OUTPUT_IDLE_CLOSE_RETRY);
                    } else {
                        self.stop_microphone_stream();
                    }
                }
            }
            (MicrophoneMode::OnDemand, MicrophoneMode::AlwaysOn) => {
                self.close_generation.fetch_add(1, Ordering::SeqCst);
                self.start_microphone_stream()?;
            }
            _ => {}
        }

        *self.mode.lock().unwrap() = new_mode;
        Ok(())
    }

    /* ---------- recording --------------------------------------------------- */

    pub fn try_start_recording(&self, binding_id: &str) -> bool {
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Idle = *state {
            // Cancel any pending lazy close and ensure the open stream still
            // points at the current effective device. This matters for the
            // "Default" mic because macOS can change it after app startup.
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            if let Err(e) = self.ensure_microphone_stream_ready() {
                error!("Failed to open microphone stream: {e}");
                let msg = e.to_string();
                let kind = if msg.to_lowercase().contains("no input device") {
                    "no_input_device"
                } else if msg.to_lowercase().contains("permission")
                    || msg.to_lowercase().contains("not permitted")
                    || msg.to_lowercase().contains("access")
                {
                    "microphone_permission_denied"
                } else {
                    "unknown"
                };
                let _ = self.app_handle.emit(
                    "recording-error",
                    serde_json::json!({ "kind": kind, "detail": msg }),
                );
                return false;
            }

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                if rec.start().is_ok() {
                    *self.is_recording.lock().unwrap() = true;
                    *state = RecordingState::Recording {
                        binding_id: binding_id.to_string(),
                    };
                    debug!("Recording started for binding {binding_id}");
                    return true;
                }
            }
            error!("Recorder not available");
            false
        } else {
            false
        }
    }

    pub fn update_selected_device(&self) -> Result<(), anyhow::Error> {
        *self.last_working_default_microphone.lock().unwrap() = None;
        let mut settings = get_settings(&self.app_handle);
        settings.last_working_default_microphone = None;
        write_settings(&self.app_handle, settings);

        // If currently open, restart the microphone stream to use the new device
        if *self.is_open.lock().unwrap() {
            self.close_generation.fetch_add(1, Ordering::SeqCst);
            self.stop_microphone_stream();
            self.start_microphone_stream()?;
        }
        Ok(())
    }

    pub fn stop_recording(&self, binding_id: &str) -> Option<Vec<f32>> {
        let mut state = self.state.lock().unwrap();

        match *state {
            RecordingState::Recording {
                binding_id: ref active,
            } if active == binding_id => {
                *state = RecordingState::Idle;
                drop(state);

                let samples = if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                    match rec.stop() {
                        Ok(buf) => buf,
                        Err(e) => {
                            error!("stop() failed: {e}");
                            Vec::new()
                        }
                    }
                } else {
                    error!("Recorder not available");
                    Vec::new()
                };

                *self.is_recording.lock().unwrap() = false;

                let settings = get_settings(&self.app_handle);
                self.remember_working_default_microphone(&settings, samples.len());

                // In on-demand mode, close the mic (lazily if the setting is enabled)
                if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                    if settings.lazy_stream_close {
                        self.schedule_lazy_close(Duration::from_secs(
                            settings.lazy_stream_close_timeout_seconds,
                        ));
                    } else {
                        self.stop_microphone_stream();
                    }
                }

                // Pad if very short
                let s_len = samples.len();
                // debug!("Got {} samples", s_len);
                if s_len < WHISPER_SAMPLE_RATE && s_len > 0 {
                    let mut padded = samples;
                    padded.resize(WHISPER_SAMPLE_RATE * 5 / 4, 0.0);
                    Some(padded)
                } else {
                    Some(samples)
                }
            }
            _ => None,
        }
    }
    pub fn is_recording(&self) -> bool {
        matches!(
            *self.state.lock().unwrap(),
            RecordingState::Recording { .. }
        )
    }

    /// Cancel any ongoing recording without returning audio samples
    pub fn cancel_recording(&self) {
        let mut state = self.state.lock().unwrap();

        if let RecordingState::Recording { .. } = *state {
            *state = RecordingState::Idle;
            drop(state);

            if let Some(rec) = self.recorder.lock().unwrap().as_ref() {
                let _ = rec.stop(); // Discard the result
            }

            *self.is_recording.lock().unwrap() = false;

            // In on-demand mode, close the mic (lazily if the setting is enabled)
            if matches!(*self.mode.lock().unwrap(), MicrophoneMode::OnDemand) {
                let settings = get_settings(&self.app_handle);
                if settings.lazy_stream_close {
                    self.schedule_lazy_close(Duration::from_secs(
                        settings.lazy_stream_close_timeout_seconds,
                    ));
                } else {
                    self.stop_microphone_stream();
                }
            }
        }
    }

    pub fn shutdown(&self) {
        debug!("Shutting down AudioRecordingManager");
        self.cancel_recording();
        self.stop_microphone_stream();
        self.remove_mute();
    }
}

impl Drop for AudioRecordingManager {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        route_names_share_headset, should_keep_idle_stream_for_shared_output,
        should_refresh_stale_stream, should_reopen_microphone_stream, STREAM_STALE_REFRESH,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn reopens_when_current_device_differs_from_desired_device() {
        assert!(should_reopen_microphone_stream(
            Some("Built-in Microphone"),
            Some("AirPods Max de Vincent")
        ));
    }

    #[test]
    fn keeps_stream_when_device_matches() {
        assert!(!should_reopen_microphone_stream(
            Some("AirPods Max de Vincent"),
            Some("AirPods Max de Vincent")
        ));
    }

    #[test]
    fn does_not_reopen_when_desired_device_is_unknown() {
        assert!(!should_reopen_microphone_stream(
            Some("AirPods Max de Vincent"),
            None
        ));
    }

    #[test]
    fn refreshes_stream_after_stale_interval() {
        let now = Instant::now();
        assert!(should_refresh_stale_stream(
            Some(now - STREAM_STALE_REFRESH - Duration::from_secs(1)),
            now
        ));
    }

    #[test]
    fn keeps_recent_stream_open() {
        let now = Instant::now();
        assert!(!should_refresh_stale_stream(
            Some(now - STREAM_STALE_REFRESH + Duration::from_secs(1)),
            now
        ));
    }

    #[test]
    fn detects_same_headset_input_and_output_route() {
        assert!(route_names_share_headset(
            "AirPods Max de Vincent",
            "AirPods Max de Vincent"
        ));
        assert!(route_names_share_headset(
            "INZONE Buds - Chat",
            "INZONE Buds - Game"
        ));
    }

    #[test]
    fn does_not_treat_unrelated_routes_as_shared_headset() {
        assert!(!route_names_share_headset(
            "AirPods Max de Vincent",
            "MacBook Pro Speakers"
        ));
        assert!(!should_keep_idle_stream_for_shared_output(
            None,
            Some("AirPods Max de Vincent")
        ));
    }

    #[test]
    fn keeps_idle_stream_for_shared_output_route() {
        assert!(should_keep_idle_stream_for_shared_output(
            Some("AirPods Max de Vincent"),
            Some("AirPods Max de Vincent")
        ));
        assert!(!should_keep_idle_stream_for_shared_output(
            Some("MacBook Pro Microphone"),
            Some("MacBook Pro Speakers")
        ));
    }
}
