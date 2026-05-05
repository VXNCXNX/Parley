use anyhow::{anyhow, Result};
use base64::Engine;
use hound::{WavSpec, WavWriter};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use log::debug;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| anyhow!("Failed to build HTTP client: {}", e))
}

/// Returns true if we should retry the request after this status code.
fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        || status == reqwest::StatusCode::GATEWAY_TIMEOUT
        || status == reqwest::StatusCode::BAD_GATEWAY
}

#[derive(Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    project_id: String,
    token_uri: String,
}

#[derive(Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    iat: u64,
    exp: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Serialize)]
struct AutoDecodingConfig {}

#[derive(Serialize)]
struct PhraseEntry {
    value: String,
    boost: f32,
}

#[derive(Serialize)]
struct InlinePhraseSet {
    phrases: Vec<PhraseEntry>,
}

#[derive(Serialize)]
struct PhraseSetEntry {
    #[serde(rename = "inlinePhraseSet")]
    inline_phrase_set: InlinePhraseSet,
}

#[derive(Serialize)]
struct SpeechAdaptation {
    #[serde(rename = "phraseSets")]
    phrase_sets: Vec<PhraseSetEntry>,
}

#[derive(Serialize)]
struct RecognitionConfig {
    #[serde(rename = "autoDecodingConfig")]
    auto_decoding_config: AutoDecodingConfig,
    #[serde(rename = "languageCodes")]
    language_codes: Vec<String>,
    model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    adaptation: Option<SpeechAdaptation>,
}

#[derive(Serialize)]
struct RecognizeRequest {
    config: RecognitionConfig,
    content: String,
}

#[derive(Deserialize, Default)]
struct RecognizeResponse {
    #[serde(default)]
    results: Vec<RecognitionResult>,
}

#[derive(Deserialize, Default)]
struct RecognitionResult {
    #[serde(default)]
    alternatives: Vec<SpeechRecognitionAlternative>,
}

#[derive(Deserialize, Default)]
struct SpeechRecognitionAlternative {
    #[serde(default)]
    transcript: String,
}

struct CachedToken {
    token: String,
    expires_at: u64,
}

static TOKEN_CACHE: Mutex<Option<CachedToken>> = Mutex::new(None);

/// Scan a window of audio and return the index of the quietest 100ms
/// segment, used as a chunk boundary that's unlikely to fall mid-word.
fn find_quiet_split(window: &[f32]) -> Option<usize> {
    const SLICE: usize = 1600; // 100ms at 16kHz
    if window.len() < SLICE {
        return None;
    }
    let mut best_idx = 0usize;
    let mut best_energy = f32::MAX;
    let mut i = 0;
    while i + SLICE <= window.len() {
        let energy: f32 = window[i..i + SLICE].iter().map(|s| s * s).sum();
        if energy < best_energy {
            best_energy = energy;
            best_idx = i + SLICE / 2;
        }
        i += SLICE;
    }
    Some(best_idx)
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

async fn get_access_token(sa: &ServiceAccountKey) -> Result<String> {
    {
        let cache = TOKEN_CACHE.lock().unwrap();
        if let Some(c) = cache.as_ref() {
            if c.expires_at > now_secs() + 60 {
                return Ok(c.token.clone());
            }
        }
    }

    let now = now_secs();
    let claims = JwtClaims {
        iss: sa.client_email.clone(),
        scope: "https://www.googleapis.com/auth/cloud-platform".to_string(),
        aud: sa.token_uri.clone(),
        iat: now,
        exp: now + 3600,
    };

    let header = Header::new(Algorithm::RS256);
    let key = EncodingKey::from_rsa_pem(sa.private_key.as_bytes())
        .map_err(|e| anyhow!("Failed to parse service account private key: {}", e))?;
    let assertion = encode(&header, &claims, &key)
        .map_err(|e| anyhow!("Failed to sign JWT: {}", e))?;

    let params = [
        ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
        ("assertion", &assertion),
    ];

    let client = build_client()?;
    let mut last_err: Option<anyhow::Error> = None;
    let mut token: Option<TokenResponse> = None;
    for (attempt, delay_ms) in [(0u32, 0u64), (1, 500), (2, 1500)] {
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        match client.post(&sa.token_uri).form(&params).send().await {
            Ok(resp) => {
                let status = resp.status();
                if status.is_success() {
                    match resp.json::<TokenResponse>().await {
                        Ok(t) => {
                            token = Some(t);
                            break;
                        }
                        Err(e) => {
                            last_err = Some(anyhow!("Failed to parse token response: {}", e));
                        }
                    }
                } else {
                    let body = resp.text().await.unwrap_or_default();
                    if is_retryable_status(status) && attempt < 2 {
                        last_err = Some(anyhow!("Token exchange transient ({}): {}", status, body));
                        continue;
                    }
                    return Err(anyhow!("Token exchange error ({}): {}", status, body));
                }
            }
            Err(e) => {
                last_err = Some(anyhow!("Token exchange request failed: {}", e));
                if attempt >= 2 {
                    break;
                }
            }
        }
    }
    let token = token.ok_or_else(|| last_err.unwrap_or_else(|| anyhow!("Token exchange failed")))?;

    let expires_at = now + token.expires_in;
    {
        let mut cache = TOKEN_CACHE.lock().unwrap();
        *cache = Some(CachedToken {
            token: token.access_token.clone(),
            expires_at,
        });
    }

    Ok(token.access_token)
}

fn encode_samples_to_wav(samples: &[f32]) -> Result<Vec<u8>> {
    let spec = WavSpec {
        channels: 1,
        sample_rate: 16000,
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

fn normalize_language(language: &str) -> String {
    // Chirp 3 expects BCP-47 codes (e.g., "fr-FR") or "auto" for multilingual.
    // Map common short codes to defaults; "auto" passes through.
    let lang = language.trim();
    if lang.is_empty() || lang.eq_ignore_ascii_case("auto") {
        return "auto".to_string();
    }
    if lang.contains('-') {
        return lang.to_string();
    }
    match lang.to_lowercase().as_str() {
        "fr" => "fr-FR",
        "en" => "en-US",
        "es" => "es-ES",
        "de" => "de-DE",
        "it" => "it-IT",
        "pt" => "pt-BR",
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "zh" => "cmn-Hans-CN",
        "ru" => "ru-RU",
        "nl" => "nl-NL",
        "pl" => "pl-PL",
        "tr" => "tr-TR",
        "ar" => "ar-XA",
        "hi" => "hi-IN",
        "vi" => "vi-VN",
        _ => "auto",
    }
    .to_string()
}

pub async fn transcribe_audio(
    service_account_json: &str,
    location: &str,
    language: &str,
    custom_words: &[String],
    audio_samples: &[f32],
) -> Result<String> {
    // Chirp 3 synchronous Recognize is capped at 60 seconds of audio.
    // For longer recordings, split into 55s chunks and concatenate transcripts.
    const SAMPLE_RATE: usize = 16000;
    const CHUNK_SECONDS: usize = 55;
    const CHUNK_SAMPLES: usize = SAMPLE_RATE * CHUNK_SECONDS;

    if audio_samples.len() > CHUNK_SAMPLES {
        debug!(
            "Chirp: audio length {} samples (~{}s) exceeds 60s limit, chunking",
            audio_samples.len(),
            audio_samples.len() / SAMPLE_RATE
        );
        let mut transcripts = Vec::new();
        let mut start = 0usize;
        while start < audio_samples.len() {
            let target_end = (start + CHUNK_SAMPLES).min(audio_samples.len());
            // Try to cut at a low-energy point in the last 5s of the window so
            // we don't slice mid-word.
            let end = if target_end == audio_samples.len() {
                target_end
            } else {
                let search_start = target_end.saturating_sub(SAMPLE_RATE * 5);
                find_quiet_split(&audio_samples[search_start..target_end])
                    .map(|i| search_start + i)
                    .unwrap_or(target_end)
            };
            let part = transcribe_chunk(
                service_account_json,
                location,
                language,
                custom_words,
                &audio_samples[start..end],
            )
            .await?;
            if !part.is_empty() {
                transcripts.push(part);
            }
            start = end;
        }
        return Ok(transcripts.join(" "));
    }

    transcribe_chunk(
        service_account_json,
        location,
        language,
        custom_words,
        audio_samples,
    )
    .await
}

async fn transcribe_chunk(
    service_account_json: &str,
    location: &str,
    language: &str,
    custom_words: &[String],
    audio_samples: &[f32],
) -> Result<String> {
    let sa: ServiceAccountKey = serde_json::from_str(service_account_json)
        .map_err(|e| anyhow!("Invalid service account JSON: {}", e))?;

    let access_token = get_access_token(&sa).await?;
    let normalized_lang = normalize_language(language);
    // Add en-US for code-switching (English terms in FR/ES/etc speech).
    let language_codes: Vec<String> = if normalized_lang.starts_with("en")
        || normalized_lang == "auto"
    {
        vec![normalized_lang]
    } else {
        vec![normalized_lang, "en-US".to_string()]
    };

    let adaptation = if custom_words.is_empty() {
        None
    } else {
        Some(SpeechAdaptation {
            phrase_sets: vec![PhraseSetEntry {
                inline_phrase_set: InlinePhraseSet {
                    phrases: custom_words
                        .iter()
                        .filter(|w| !w.trim().is_empty())
                        .map(|w| PhraseEntry {
                            value: w.clone(),
                            boost: 10.0,
                        })
                        .collect(),
                },
            }],
        })
    };

    let wav_bytes = encode_samples_to_wav(audio_samples)?;
    let audio_base64 = base64::engine::general_purpose::STANDARD.encode(&wav_bytes);

    debug!(
        "Chirp transcribe: {} samples, {} bytes WAV, location={}, project={}",
        audio_samples.len(),
        wav_bytes.len(),
        location,
        sa.project_id
    );

    let url = format!(
        "https://{}-speech.googleapis.com/v2/projects/{}/locations/{}/recognizers/_:recognize",
        location, sa.project_id, location
    );

    let request = RecognizeRequest {
        config: RecognitionConfig {
            auto_decoding_config: AutoDecodingConfig {},
            language_codes,
            model: "chirp_3".to_string(),
            adaptation,
        },
        content: audio_base64,
    };

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", access_token))
            .map_err(|e| anyhow!("Invalid bearer token: {}", e))?,
    );

    let client = build_client()?;
    let mut last_err: Option<anyhow::Error> = None;
    let mut resp: Option<RecognizeResponse> = None;
    for (attempt, delay_ms) in [(0u32, 0u64), (1, 500), (2, 1500)] {
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }
        match client
            .post(&url)
            .headers(headers.clone())
            .json(&request)
            .send()
            .await
        {
            Ok(response) => {
                let status = response.status();
                if status.is_success() {
                    match response.json::<RecognizeResponse>().await {
                        Ok(r) => {
                            resp = Some(r);
                            break;
                        }
                        Err(e) => {
                            return Err(anyhow!("Failed to parse Chirp response: {}", e));
                        }
                    }
                } else {
                    let error_text = response.text().await.unwrap_or_default();
                    if is_retryable_status(status) && attempt < 2 {
                        last_err = Some(anyhow!("Chirp transient ({}): {}", status, error_text));
                        continue;
                    }
                    return Err(anyhow!("Chirp API error ({}): {}", status, error_text));
                }
            }
            Err(e) => {
                last_err = Some(anyhow!("Chirp request failed: {}", e));
                if attempt >= 2 {
                    break;
                }
            }
        }
    }
    let resp = resp.ok_or_else(|| last_err.unwrap_or_else(|| anyhow!("Chirp request failed")))?;

    let text = resp
        .results
        .into_iter()
        .filter_map(|r| r.alternatives.into_iter().next())
        .map(|a| a.transcript)
        .collect::<Vec<_>>()
        .join(" ");

    debug!("Chirp transcription result: {}", text);
    Ok(text.trim().to_string())
}
