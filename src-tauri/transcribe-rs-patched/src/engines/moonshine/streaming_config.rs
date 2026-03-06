use std::fs;
use std::path::Path;

use super::model::MoonshineError;

/// Streaming model configuration parsed from `streaming_config.json`.
#[derive(Debug, Clone)]
pub struct StreamingConfig {
    pub encoder_dim: usize,
    pub decoder_dim: usize,
    pub depth: usize,
    pub nheads: usize,
    pub head_dim: usize,
    pub vocab_size: usize,
    pub bos_id: i64,
    pub eos_id: i64,
    pub frame_len: usize,
    pub total_lookahead: usize,
    pub d_model_frontend: usize,
    pub c1: usize,
    pub c2: usize,
    pub max_seq_len: usize,
}

impl StreamingConfig {
    /// Load streaming config from `streaming_config.json` in the model directory.
    pub fn load(model_dir: &Path) -> Result<Self, MoonshineError> {
        let config_path = model_dir.join("streaming_config.json");
        if !config_path.exists() {
            return Err(MoonshineError::ModelNotFound(
                config_path.display().to_string(),
            ));
        }

        let contents = fs::read_to_string(&config_path).map_err(MoonshineError::Io)?;
        let json: serde_json::Value = serde_json::from_str(&contents).map_err(|e| {
            MoonshineError::Tokenization(format!("Failed to parse streaming_config.json: {}", e))
        })?;

        let get_usize = |key: &str| -> usize {
            json.get(key)
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as usize
        };

        let get_i64 = |key: &str| -> i64 {
            json.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
        };

        let max_seq_len = {
            let v = get_usize("max_seq_len");
            if v > 0 { v } else { 448 }
        };

        let config = StreamingConfig {
            encoder_dim: get_usize("encoder_dim"),
            decoder_dim: get_usize("decoder_dim"),
            depth: get_usize("depth"),
            nheads: get_usize("nheads"),
            head_dim: get_usize("head_dim"),
            vocab_size: get_usize("vocab_size"),
            bos_id: get_i64("bos_id"),
            eos_id: get_i64("eos_id"),
            frame_len: get_usize("frame_len"),
            total_lookahead: get_usize("total_lookahead"),
            d_model_frontend: get_usize("d_model_frontend"),
            c1: get_usize("c1"),
            c2: get_usize("c2"),
            max_seq_len,
        };

        if config.depth == 0 || config.decoder_dim == 0 || config.vocab_size == 0 {
            return Err(MoonshineError::Tokenization(
                "Invalid streaming config: depth, decoder_dim, and vocab_size must be > 0"
                    .to_string(),
            ));
        }

        log::info!("Loaded streaming config: {:?}", config);

        Ok(config)
    }
}
