use anyhow::{anyhow, Result};
use std::path::Path;
use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions, Transcript};

const NATIVE_RUNTIME_VERSION: &str = "0.2.3";
const NEMOTRON_BATCH_FIXTURE_SHA256: &str =
    "b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c";
const NEMOTRON_BATCH_MARKER: &str = "Bonjour";

pub fn native_runtime_version() -> &'static str {
    NATIVE_RUNTIME_VERSION
}

pub fn load_native_model(path: &Path) -> Result<Model> {
    let linked = transcribe_cpp::version();
    if linked != NATIVE_RUNTIME_VERSION {
        return Err(anyhow!(
            "transcribe-cpp linked {linked}, expected {NATIVE_RUNTIME_VERSION}"
        ));
    }
    transcribe_cpp::init_backends_default()?;
    let backend = if cfg!(target_os = "macos") {
        Backend::Metal
    } else {
        Backend::Auto
    };
    Model::load_with(path, &ModelOptions {
        backend,
        ..Default::default()
    })
    .map_err(|error| anyhow!("load {}: {error}", path.display()))
}

pub fn transcribe_native_batch(model: &Model, pcm: &[f32]) -> Result<Transcript> {
    let mut session = model
        .session()
        .map_err(|error| anyhow!("open native session: {error}"))?;
    session
        .run(pcm, &RunOptions::default())
        .map_err(|error| anyhow!("native batch transcription: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::path::PathBuf;

    fn fixture_paths() -> Option<(PathBuf, PathBuf)> {
        let root = std::env::var_os("PARLEY_NATIVE_FIXTURE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/parley-handy-model-fixture"));
        let model = root.join("nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf");
        let audio = root.join("french-s16.wav");
        (model.is_file() && audio.is_file()).then_some((model, audio))
    }

    fn read_pcm(path: &Path) -> Vec<f32> {
        let mut reader = hound::WavReader::open(path).unwrap();
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, 16_000);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);
        reader
            .samples::<i16>()
            .map(|sample| sample.unwrap() as f32 / 32768.0)
            .collect()
    }

    #[test]
    fn native_runtime_reports_pinned_version() {
        assert_eq!(native_runtime_version(), "0.2.3");
        assert_eq!(transcribe_cpp::compiled_version(), "0.2.3");
    }

    #[test]
    fn nemotron_metal_batch_returns_full_fixture_text() {
        let Some((model_path, audio_path)) = fixture_paths() else {
            eprintln!("native fixture absent, skipping real model batch");
            return;
        };
        let bytes = std::fs::read(&model_path).unwrap();
        let digest = format!("{:x}", Sha256::digest(bytes));
        assert_eq!(digest, NEMOTRON_BATCH_FIXTURE_SHA256);

        let model = load_native_model(&model_path).unwrap();
        if cfg!(target_os = "macos") {
            assert_eq!(model.backend(), "MTL0");
        }
        assert!(model.capabilities().supports_streaming);
        let pcm = read_pcm(&audio_path);
        assert!(pcm.len() > 16_000 * 7);

        let transcript = transcribe_native_batch(&model, &pcm).unwrap();
        assert!(
            transcript.text.contains(NEMOTRON_BATCH_MARKER),
            "batch text missing opening word: {}",
            transcript.text
        );
        assert!(
            transcript.text.split_whitespace().count() > 8,
            "batch text is only a fragment: {}",
            transcript.text
        );
    }
}
