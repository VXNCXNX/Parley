use std::path::PathBuf;
use std::time::Instant;

use transcribe_rs::{
    engines::sense_voice::{SenseVoiceEngine, SenseVoiceModelParams},
    TranscriptionEngine,
};

fn get_audio_duration(path: &PathBuf) -> Result<f64, Box<dyn std::error::Error>> {
    let reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let duration = reader.duration() as f64 / spec.sample_rate as f64;
    Ok(duration)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    let args: Vec<String> = std::env::args().collect();
    let int8 = args.iter().any(|a| a == "--int8");
    let positional: Vec<&String> = args
        .iter()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .collect();

    let model_path = PathBuf::from(
        positional
            .first()
            .map(|s| s.as_str())
            .unwrap_or("models/sense-voice"),
    );
    let wav_path = PathBuf::from(
        positional
            .get(1)
            .map(|s| s.as_str())
            .unwrap_or("samples/dots.wav"),
    );

    let model_params = if int8 {
        SenseVoiceModelParams::int8()
    } else {
        SenseVoiceModelParams::fp32()
    };

    let audio_duration = get_audio_duration(&wav_path)?;
    println!("Audio duration: {:.2}s", audio_duration);

    println!("Using SenseVoice engine");
    println!(
        "Loading model: {:?} (quantization: {})",
        model_path,
        if int8 { "int8" } else { "fp32" }
    );

    let mut engine = SenseVoiceEngine::new();
    let load_start = Instant::now();
    engine.load_model_with_params(&model_path, model_params)?;
    let load_duration = load_start.elapsed();
    println!("Model loaded in {:.2?}", load_duration);

    println!("Transcribing file: {:?}", wav_path);
    let transcribe_start = Instant::now();

    let result = engine.transcribe_file(&wav_path, None)?;
    let transcribe_duration = transcribe_start.elapsed();
    println!("Transcription completed in {:.2?}", transcribe_duration);

    let speedup_factor = audio_duration / transcribe_duration.as_secs_f64();
    println!(
        "Real-time speedup: {:.2}x faster than real-time",
        speedup_factor
    );

    println!("Transcription result:");
    println!("{}", result.text);

    if let Some(segments) = result.segments {
        println!("\nSegments:");
        for segment in segments {
            println!(
                "[{:.2}s - {:.2}s]: {}",
                segment.start, segment.end, segment.text
            );
        }
    }

    engine.unload_model();

    Ok(())
}
