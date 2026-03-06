//! SenseVoice ONNX transcription engine.
//!
//! This module provides transcription using the SenseVoice/FunASR model via ONNX Runtime.
//! SenseVoice is a CTC-based speech recognition model with built-in language detection,
//! emotion recognition, and audio event detection.
//!
//! # Model Architecture
//!
//! SenseVoice uses a CTC encoder with special prefix tokens:
//! - Processes audio via FBANK features → LFR stacking → CMVN normalization
//! - Outputs include language, emotion, and event classification alongside speech text
//!
//! # Model Format
//!
//! Expects a directory containing:
//! - `model.onnx` - The SenseVoice encoder model
//! - `tokens.txt` - Token vocabulary (ID-to-symbol mapping)
//!
//! # Supported Languages
//!
//! Chinese (Mandarin), English, Japanese, Korean, Cantonese, or auto-detect.
//!
//! # Audio Requirements
//!
//! - Sample rate: 16 kHz
//! - Format: Mono, 16-bit PCM
//!
//! # Example
//!
//! ```rust,no_run
//! use std::path::PathBuf;
//! use transcribe_rs::{TranscriptionEngine, engines::sense_voice::{SenseVoiceEngine, SenseVoiceModelParams}};
//!
//! let mut engine = SenseVoiceEngine::new();
//! engine.load_model_with_params(
//!     &PathBuf::from("models/sense-voice"),
//!     SenseVoiceModelParams::default(),
//! )?;
//!
//! let result = engine.transcribe_file(&PathBuf::from("audio.wav"), None)?;
//! println!("Transcription: {}", result.text);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

pub mod decoder;
pub mod engine;
pub mod features;
pub mod model;
mod tokens;

pub use engine::{
    Language, QuantizationType, SenseVoiceEngine, SenseVoiceInferenceParams, SenseVoiceModelParams,
};
pub use model::SenseVoiceError;
