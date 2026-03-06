use std::path::{Path, PathBuf};

use crate::{TranscriptionEngine, TranscriptionResult};

use super::streaming_model::StreamingModel;

const SAMPLE_RATE: u32 = 16000;

/// Parameters for loading a streaming Moonshine model.
#[derive(Debug, Clone)]
pub struct StreamingModelParams {
    /// Maximum tokens generated per second of audio. Default: 6.5.
    pub max_tokens_per_second: f32,
    /// Number of intra-op threads for ONNX Runtime. 0 = let ORT decide (typically num cores).
    pub num_threads: usize,
}

impl Default for StreamingModelParams {
    fn default() -> Self {
        Self {
            max_tokens_per_second: 6.5,
            num_threads: 0,
        }
    }
}

/// Parameters for streaming inference.
#[derive(Debug, Clone, Default)]
pub struct StreamingInferenceParams {
    /// Maximum number of tokens to generate.
    /// If None, automatically calculated from audio duration.
    pub max_length: Option<usize>,
}

/// Streaming Moonshine transcription engine.
///
/// Uses the 5-session streaming ONNX pipeline (frontend, encoder, adapter,
/// cross_kv, decoder_kv) for transcription. Currently operates in offline
/// (batch) mode, structured for a future streaming API.
pub struct MoonshineStreamingEngine {
    model: Option<StreamingModel>,
    loaded_model_path: Option<PathBuf>,
    max_tokens_per_second: f32,
}

impl MoonshineStreamingEngine {
    /// Create a new streaming engine (model not loaded).
    pub fn new() -> Self {
        Self {
            model: None,
            loaded_model_path: None,
            max_tokens_per_second: StreamingModelParams::default().max_tokens_per_second,
        }
    }
}

impl Default for MoonshineStreamingEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MoonshineStreamingEngine {
    fn drop(&mut self) {
        self.unload_model();
    }
}

impl TranscriptionEngine for MoonshineStreamingEngine {
    type InferenceParams = StreamingInferenceParams;
    type ModelParams = StreamingModelParams;

    fn load_model_with_params(
        &mut self,
        model_path: &Path,
        params: Self::ModelParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.unload_model();

        self.max_tokens_per_second = params.max_tokens_per_second;
        self.model = Some(StreamingModel::new(model_path, params.num_threads)?);
        self.loaded_model_path = Some(model_path.to_path_buf());

        log::info!(
            "Loaded Moonshine streaming model from {:?}",
            model_path
        );

        Ok(())
    }

    fn unload_model(&mut self) {
        if self.model.is_some() {
            log::debug!("Unloading Moonshine streaming model");
            self.model = None;
            self.loaded_model_path = None;
        }
    }

    fn transcribe_samples(
        &mut self,
        samples: Vec<f32>,
        params: Option<Self::InferenceParams>,
    ) -> Result<TranscriptionResult, Box<dyn std::error::Error>> {
        let model = self
            .model
            .as_mut()
            .ok_or("Streaming model not loaded")?;

        let max_tokens_override = params.and_then(|p| p.max_length);

        log::debug!(
            "Transcribing {} samples ({:.2}s) with streaming model",
            samples.len(),
            samples.len() as f32 / SAMPLE_RATE as f32,
        );

        let tokens = model.generate(&samples, self.max_tokens_per_second, max_tokens_override)?;
        let text = model.decode_tokens(&tokens)?;

        Ok(TranscriptionResult {
            text,
            segments: None,
        })
    }
}
