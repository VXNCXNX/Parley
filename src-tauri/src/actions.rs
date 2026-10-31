use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::cleanup;
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::transcription::TranscriptionManager;
use crate::settings::{get_settings, AppSettings};
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils::{
    self, show_processing_overlay, show_recording_overlay, show_transcribing_overlay,
};
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use log::{debug, error, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

// Transcribe Action
struct TranscribeAction;

/// Run the AI cleanup pass, or `None` when the raw transcript should be used as is.
async fn clean_up_with_ai(
    app: &AppHandle,
    settings: &AppSettings,
    transcription: &str,
) -> Option<String> {
    let Some(api_key) = settings.get_decrypted_gemini_api_key() else {
        debug!("AI cleanup skipped: no Gemini API key");
        return None;
    };

    show_processing_overlay(app);

    match cleanup::clean_transcript(&api_key, &settings.custom_words, transcription).await {
        Ok(cleaned) => Some(cleaned),
        Err(reason) => {
            debug!("AI cleanup skipped: {reason}");
            None
        }
    }
}

async fn maybe_convert_chinese_variant(
    settings: &AppSettings,
    transcription: &str,
) -> Option<String> {
    // Check if language is set to Simplified or Traditional Chinese
    let is_simplified = settings.selected_language == "zh-Hans";
    let is_traditional = settings.selected_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("selected_language is not Simplified or Traditional Chinese; skipping translation");
        return None;
    }

    debug!(
        "Starting Chinese translation using OpenCC for language: {}",
        settings.selected_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2twp
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        tm.initiate_model_load();

        let binding_id = binding_id.to_string();
        change_tray_icon(app, TrayIconState::Recording);
        show_recording_overlay(app);

        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Get the microphone mode to determine audio feedback timing
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_started = false;
        rm.set_dictation_feed(None);
        if let Some(feed) = tm.begin_dictation() {
            rm.set_dictation_feed(Some(feed));
        }
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            // The blocking helper exits immediately if audio feedback is disabled,
            // so we can always reuse this thread to ensure mute happens right after playback.
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });

            recording_started = rm.try_start_recording(&binding_id);
            debug!("Recording started: {}", recording_started);
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            if rm.try_start_recording(&binding_id) {
                recording_started = true;
                debug!("Recording started in {:?}", recording_start_time.elapsed());
                // Small delay to ensure microphone stream is active
                let app_clone = app.clone();
                let rm_clone = Arc::clone(&rm);
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    debug!("Handling delayed audio feedback/mute sequence");
                    // Helper handles disabled audio feedback by returning early, so we reuse it
                    // to keep mute sequencing consistent in every mode.
                    play_feedback_sound_blocking(&app_clone, SoundType::Start);
                    rm_clone.apply_mute();
                });
            } else {
                debug!("Failed to start recording");
            }
        }

        if !recording_started {
            rm.set_dictation_feed(None);
            tm.cancel_dictation();
        }

        if recording_started {
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        show_transcribing_overlay(app);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            let binding_id = binding_id.clone(); // Clone for the inner async task
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            let stopped = rm.stop_recording(&binding_id);
            // Unmute and play the stop sound only once the release tail is captured,
            // so neither the tone nor unmuted audio ends up in the recording.
            rm.remove_mute();
            play_feedback_sound(&ah, SoundType::Stop);
            if let Some((samples, used_raw_fallback)) = stopped {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                let duration_seconds = samples.len() as f32 / 16000.0;
                let settings_for_model = get_settings(&ah);
                let original_model = tm.get_current_model();
                let mut switched_model = false;

                if let Some(ref long_model_id) = settings_for_model.long_audio_model {
                    if duration_seconds > settings_for_model.long_audio_threshold_seconds
                        && original_model.as_deref() != Some(long_model_id.as_str())
                    {
                        // Pre-flight: don't switch to a cloud model the user hasn't configured.
                        // Without this, we'd swap to gemini-api and fail at transcribe time
                        // with a confusing error, leaving the overlay stuck.
                        let is_gemini = long_model_id == "gemini-api";
                        let needs_chirp_sa = settings_for_model.gemini_model.starts_with("chirp");
                        let missing_credentials = if is_gemini {
                            settings_for_model.get_decrypted_gemini_api_key().is_none()
                                || (needs_chirp_sa
                                    && settings_for_model
                                        .get_decrypted_chirp_service_account()
                                        .is_none())
                        } else {
                            false
                        };
                        if missing_credentials {
                            warn!(
                                "Long audio model '{}' is not fully configured (missing API key or service account); staying on current model",
                                long_model_id
                            );
                        } else {
                            debug!(
                                "Audio duration {:.1}s exceeds threshold {:.1}s, switching to long audio model: {}",
                                duration_seconds,
                                settings_for_model.long_audio_threshold_seconds,
                                long_model_id
                            );
                            if let Err(e) = tm.load_model(long_model_id) {
                                warn!(
                                    "Failed to load long audio model '{}': {}, using current model",
                                    long_model_id, e
                                );
                            } else {
                                switched_model = true;
                            }
                        }
                    }
                }

                let transcription_time = Instant::now();
                let samples_clone = samples.clone(); // Clone for history saving
                let feed = rm.take_dictation_feed();
                rm.set_dictation_feed(None);
                let transcription_result = if switched_model || used_raw_fallback {
                    tm.cancel_dictation();
                    tm.transcribe(samples)
                } else {
                    tm.finish_recorded_dictation(samples, feed)
                };
                match transcription_result {
                    Ok(transcription) => {
                        debug!(
                            "Transcription completed in {:?}: '{}'",
                            transcription_time.elapsed(),
                            transcription
                        );
                        if !transcription.is_empty() {
                            let settings = get_settings(&ah);
                            let mut final_text = transcription.clone();

                            // First, check if Chinese variant conversion is needed
                            if let Some(converted_text) =
                                maybe_convert_chinese_variant(&settings, &transcription).await
                            {
                                final_text = converted_text;
                            }

                            if settings.post_process_enabled {
                                if let Some(cleaned_text) =
                                    clean_up_with_ai(&ah, &settings, &final_text).await
                                {
                                    final_text = cleaned_text;
                                }
                            }

                            let post_processed_text =
                                (final_text != transcription).then(|| final_text.clone());

                            // Save to history with the processed text, if any
                            let hm_clone = Arc::clone(&hm);
                            let transcription_for_history = transcription.clone();
                            tauri::async_runtime::spawn(async move {
                                if let Err(e) = hm_clone
                                    .save_transcription(
                                        samples_clone,
                                        transcription_for_history,
                                        post_processed_text,
                                    )
                                    .await
                                {
                                    error!("Failed to save transcription to history: {}", e);
                                }
                            });

                            // Paste the final text (either processed or original)
                            let ah_clone = ah.clone();
                            let paste_time = Instant::now();
                            ah.run_on_main_thread(move || {
                                match utils::paste(final_text, ah_clone.clone()) {
                                    Ok(()) => debug!(
                                        "Text pasted successfully in {:?}",
                                        paste_time.elapsed()
                                    ),
                                    Err(e) => error!("Failed to paste transcription: {}", e),
                                }
                                // Hide the overlay after transcription is complete
                                utils::hide_recording_overlay(&ah_clone);
                                change_tray_icon(&ah_clone, TrayIconState::Idle);
                            })
                            .unwrap_or_else(|e| {
                                error!("Failed to run paste on main thread: {:?}", e);
                                utils::hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                            });
                        } else {
                            utils::hide_recording_overlay(&ah);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                    Err(err) => {
                        debug!("Global Shortcut Transcription error: {}", err);
                        let _ = ah.emit("transcription-error", err.to_string());
                        utils::hide_recording_overlay(&ah);
                        change_tray_icon(&ah, TrayIconState::Idle);
                    }
                }

                // Restore original model if we switched for long audio
                if switched_model {
                    if let Some(ref orig_id) = original_model {
                        debug!("Restoring original model: {}", orig_id);
                        if let Err(e) = tm.load_model(orig_id) {
                            warn!("Failed to restore original model '{}': {}", orig_id, e);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                utils::hide_recording_overlay(&ah);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});
