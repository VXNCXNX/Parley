//! Client for Google's dedicated speech-to-text model `gemini-3.5-transcribe`,
//! served by the Gemini Interactions API.
//!
//! Audio is sent inline (base64 WAV), no Files API upload is needed.

use anyhow::{anyhow, Result};
use base64::Engine;
use hound::{WavSpec, WavWriter};
use log::debug;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::time::Duration;

const INTERACTIONS_URL: &str = "https://generativelanguage.googleapis.com/v1beta/interactions";
pub const MODEL_ID: &str = "gemini-3.5-transcribe";

const SAMPLE_RATE: usize = 16000;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const BASE_TIMEOUT_SECS: u64 = 30;
/// The model recommends at most ~100 vocabulary terms; more dilutes the bias.
const MAX_VOCABULARY_TERMS: usize = 100;

#[derive(Serialize)]
struct AudioInput {
    #[serde(rename = "type")]
    kind: &'static str,
    data: String,
    mime_type: &'static str,
}

#[derive(Serialize)]
struct TranscriptionConfig {
    mode: &'static str,
    language_codes: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    custom_vocabulary: Vec<String>,
}

#[derive(Serialize)]
struct GenerationConfig {
    transcription_config: TranscriptionConfig,
}

#[derive(Serialize)]
struct InteractionRequest {
    model: &'static str,
    input: Vec<AudioInput>,
    generation_config: GenerationConfig,
    /// Do not keep the recording or transcript on Google's side.
    store: bool,
}

#[derive(Deserialize, Default)]
struct InteractionResponse {
    /// `completed` when the transcript is whole. `incomplete` (e.g. an output-token limit),
    /// `failed`, `cancelled` and the continuation states mean the text can't be trusted.
    #[serde(default)]
    status: String,
    #[serde(default)]
    steps: Vec<Step>,
}

#[derive(Deserialize, Default)]
struct Step {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    content: Vec<Content>,
}

#[derive(Deserialize, Default)]
struct Content {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
}

fn encode_samples_to_wav(samples: &[f32]) -> Result<Vec<u8>> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE as u32,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = WavWriter::new(cursor, spec)?;
        for sample in samples {
            let sample_i16 = (sample * i16::MAX as f32) as i16;
            writer.write_sample(sample_i16)?;
        }
        writer.finalize()?;
    }
    Ok(buffer)
}

/// Returns true if we should retry the request after this status code.
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

/// Empty means automatic detection, which also handles code-switching.
fn language_codes(language: &str) -> Vec<String> {
    let lang = language.trim();
    if lang.is_empty() || lang.eq_ignore_ascii_case("auto") {
        Vec::new()
    } else {
        vec![lang.to_string()]
    }
}

fn vocabulary(custom_words: &[String]) -> Vec<String> {
    custom_words
        .iter()
        .map(|w| w.trim())
        .filter(|w| !w.is_empty())
        .take(MAX_VOCABULARY_TERMS)
        .map(str::to_string)
        .collect()
}

fn build_client(audio_seconds: usize) -> Result<reqwest::Client> {
    // Longer recordings take longer to process and upload.
    let timeout = Duration::from_secs(BASE_TIMEOUT_SECS + (audio_seconds / 4) as u64);
    reqwest::Client::builder()
        .timeout(timeout)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| anyhow!("Failed to build HTTP client: {}", e))
}

/// Transcribe 16 kHz mono samples with `gemini-3.5-transcribe` in smart mode.
///
/// `language` is the app language setting ("auto" lets the model detect it).
/// `custom_words` biases recognition toward names and jargon (first 100 used).
pub async fn transcribe_audio(
    api_key: &str,
    language: &str,
    custom_words: &[String],
    audio_samples: &[f32],
) -> Result<String> {
    let wav_bytes = encode_samples_to_wav(audio_samples)?;
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);

    debug!(
        "Gemini transcribe: {} samples, {} bytes WAV, {} bytes base64",
        audio_samples.len(),
        wav_bytes.len(),
        audio_base64.len()
    );

    let request = InteractionRequest {
        model: MODEL_ID,
        input: vec![AudioInput {
            kind: "audio",
            data: audio_base64,
            mime_type: "audio/wav",
        }],
        generation_config: GenerationConfig {
            transcription_config: TranscriptionConfig {
                mode: "smart",
                language_codes: language_codes(language),
                custom_vocabulary: vocabulary(custom_words),
            },
        },
        store: false,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-goog-api-key",
        HeaderValue::from_str(api_key).map_err(|e| anyhow!("Invalid API key: {}", e))?,
    );

    let client = build_client(audio_samples.len() / SAMPLE_RATE)?;
    let mut last_err: Option<anyhow::Error> = None;
    let mut resp: Option<InteractionResponse> = None;
    for (attempt, delay_ms) in [(0u32, 0u64), (1, 500), (2, 1500)] {
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        match client
            .post(INTERACTIONS_URL)
            .headers(headers.clone())
            .json(&request)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    match response.json::<InteractionResponse>().await {
                        Ok(r) => {
                            resp = Some(r);
                            break;
                        }
                        Err(e) => {
                            return Err(anyhow!("Failed to parse Gemini response: {}", e));
                        }
                    }
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    if is_retryable_status(status) && attempt < 2 {
                        last_err = Some(anyhow!(
                            "Gemini transcribe transient ({}): {}",
                            status,
                            error_text
                        ));
                        continue;
                    }
                    return Err(anyhow!("Gemini API error ({}): {}", status, error_text));
                }
            }
            Err(e) => {
                last_err = Some(anyhow!("Gemini request failed: {}", e));
                if attempt >= 2 {
                    break;
                }
            }
        }
    }
    let resp = resp.ok_or_else(|| last_err.unwrap_or_else(|| anyhow!("Gemini request failed")))?;
    if resp.status != "completed" {
        return Err(anyhow!(
            "Gemini transcription ended with status '{}', not 'completed'",
            resp.status
        ));
    }

    let text = resp
        .steps
        .into_iter()
        .filter(|s| s.kind == "model_output")
        .flat_map(|s| s.content)
        .filter(|c| c.kind == "text")
        .map(|c| c.text)
        .collect::<Vec<_>>()
        .join("");

    debug!("Gemini transcription result: {}", text);
    Ok(text.trim().to_string())
}
