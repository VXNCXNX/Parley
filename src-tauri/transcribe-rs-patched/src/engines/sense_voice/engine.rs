use std::path::{Path, PathBuf};

use crate::{TranscriptionEngine, TranscriptionResult, TranscriptionSegment};

use super::model::{SenseVoiceError, SenseVoiceModel};

/// Supported language options for SenseVoice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    /// Auto-detect language.
    Auto,
    /// Chinese (Mandarin).
    Chinese,
    /// English.
    English,
    /// Japanese.
    Japanese,
    /// Korean.
    Korean,
    /// Cantonese.
    Cantonese,
}

impl Language {
    fn as_str(&self) -> &str {
        match self {
            Language::Auto => "auto",
            Language::Chinese => "zh",
            Language::English => "en",
            Language::Japanese => "ja",
            Language::Korean => "ko",
            Language::Cantonese => "yue",
        }
    }
}

impl Default for Language {
    fn default() -> Self {
        Language::Auto
    }
}

/// Quantization type for SenseVoice model loading.
///
/// Controls the precision/performance trade-off for the loaded model.
/// Int8 quantization provides faster inference at the cost of some accuracy.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum QuantizationType {
    /// Full precision ONNX model (`model.onnx`)
    #[default]
    FP32,
    /// 8-bit integer quantized model (`model.int8.onnx`)
    Int8,
}

/// Parameters for loading a SenseVoice model.
#[derive(Debug, Clone, Default)]
pub struct SenseVoiceModelParams {
    /// The quantization type to use for the model.
    pub quantization: QuantizationType,
}

impl SenseVoiceModelParams {
    /// Create parameters for full precision (FP32) model loading.
    pub fn fp32() -> Self {
        Self {
            quantization: QuantizationType::FP32,
        }
    }

    /// Create parameters for Int8 quantized model loading.
    pub fn int8() -> Self {
        Self {
            quantization: QuantizationType::Int8,
        }
    }
}

/// Parameters for SenseVoice inference.
#[derive(Debug, Clone)]
pub struct SenseVoiceInferenceParams {
    /// Language to use for transcription.
    pub language: Language,
    /// Whether to apply inverse text normalization.
    pub use_itn: bool,
}

impl Default for SenseVoiceInferenceParams {
    fn default() -> Self {
        Self {
            language: Language::Auto,
            use_itn: true,
        }
    }
}

/// SenseVoice ONNX transcription engine.
///
/// Implements the `TranscriptionEngine` trait for SenseVoice models.
/// Supports multilingual transcription with language/emotion/event detection.
pub struct SenseVoiceEngine {
    loaded_model_path: Option<PathBuf>,
    model: Option<SenseVoiceModel>,
}

impl SenseVoiceEngine {
    /// Create a new SenseVoice engine (model not loaded).
    pub fn new() -> Self {
        Self {
            loaded_model_path: None,
            model: None,
        }
    }
}

impl Default for SenseVoiceEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SenseVoiceEngine {
    fn drop(&mut self) {
        self.unload_model();
    }
}

impl TranscriptionEngine for SenseVoiceEngine {
    type InferenceParams = SenseVoiceInferenceParams;
    type ModelParams = SenseVoiceModelParams;

    fn load_model_with_params(
        &mut self,
        model_path: &Path,
        params: Self::ModelParams,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.unload_model();

        let quantized = matches!(params.quantization, QuantizationType::Int8);
        self.model = Some(SenseVoiceModel::new(model_path, quantized)?);
        self.loaded_model_path = Some(model_path.to_path_buf());

        log::info!("Loaded SenseVoice model from {:?}", model_path);
        Ok(())
    }

    fn unload_model(&mut self) {
        if self.model.is_some() {
            log::debug!("Unloading SenseVoice model");
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
            .ok_or_else(|| SenseVoiceError::ModelNotLoaded)?;

        let params = params.unwrap_or_default();

        log::debug!(
            "Transcribing {} samples ({:.2}s), language={:?}, use_itn={}",
            samples.len(),
            samples.len() as f32 / 16000.0,
            params.language,
            params.use_itn,
        );

        let result = model.transcribe(&samples, params.language.as_str(), params.use_itn)?;

        // Convert token-level timestamps to segments
        // Group tokens into segments (each token is its own segment for now)
        let segments = if !result.timestamps.is_empty() {
            let mut segs = Vec::new();
            for (i, token) in result.tokens.iter().enumerate() {
                let start = result.timestamps.get(i).copied().unwrap_or(0.0);
                let end = result
                    .timestamps
                    .get(i + 1)
                    .copied()
                    .unwrap_or(start + 0.06); // ~1 LFR frame
                segs.push(TranscriptionSegment {
                    start,
                    end,
                    text: token.clone(),
                });
            }
            Some(segs)
        } else {
            None
        };

        Ok(TranscriptionResult {
            text: result.text,
            segments,
        })
    }
}
