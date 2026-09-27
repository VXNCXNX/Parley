use anyhow::{anyhow, Result};
use libloading::Library;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const NATIVE_LIBRARY_NAME: &str = "libparley_native_runtime.dylib";

type TranscribeBatch = unsafe extern "C" fn(
    *const c_char,
    *const f32,
    usize,
    *const c_char,
    c_int,
    *mut *mut c_char,
    *mut *mut c_char,
) -> c_int;
type StringFree = unsafe extern "C" fn(*mut c_char);

struct NativeApi {
    _library: Library,
    transcribe_batch: TranscribeBatch,
    string_free: StringFree,
}

fn library_path() -> PathBuf {
    if let Some(path) = std::env::var_os("PARLEY_NATIVE_RUNTIME_PATH") {
        return PathBuf::from(path);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("native-runtime/target/release")
        .join(NATIVE_LIBRARY_NAME)
}

fn api() -> Result<&'static NativeApi> {
    static API: OnceLock<NativeApi> = OnceLock::new();
    if API.get().is_some() {
        return Ok(API.get().unwrap());
    }
    let path = library_path();
    let library = unsafe { Library::new(&path) }.map_err(|error| anyhow!("load {}: {error}", path.display()))?;
    let transcribe_batch = *unsafe {
        library.get::<TranscribeBatch>(b"parley_native_transcribe_batch\x00")
    }.map_err(|error| anyhow!("native batch symbol: {error}"))?;
    let string_free = *unsafe {
        library.get::<StringFree>(b"parley_native_string_free\x00")
    }.map_err(|error| anyhow!("native free symbol: {error}"))?;
    let _ = API.set(NativeApi {
        _library: library,
        transcribe_batch,
        string_free,
    });
    Ok(API.get().unwrap())
}

pub fn native_runtime_version() -> &'static str {
    "0.2.3"
}

pub fn transcribe_native_batch(path: &Path, pcm: &[f32], language: Option<&str>, live_backend: bool) -> Result<String> {
    let api = api()?;
    let path = CString::new(path.to_string_lossy().as_bytes()).map_err(|_| anyhow!("model path contains nul"))?;
    let language = language.map(CString::new).transpose().map_err(|_| anyhow!("language contains nul"))?;
    let mut text_out = std::ptr::null_mut();
    let mut error_out = std::ptr::null_mut();
    let status = unsafe {
        (api.transcribe_batch)(
            path.as_ptr(),
            pcm.as_ptr(),
            pcm.len(),
            language.as_ref().map_or(std::ptr::null(), |value| value.as_ptr()),
            c_int::from(live_backend),
            &mut text_out,
            &mut error_out,
        )
    };
    let text = take_string(api, text_out);
    let error = take_string(api, error_out);
    if status != 0 {
        return Err(anyhow!(error.unwrap_or_else(|| "native transcription failed".to_string())));
    }
    text.ok_or_else(|| anyhow!("native transcription returned no text"))
}

fn take_string(api: &NativeApi, value: *mut c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    let text = unsafe { CStr::from_ptr(value) }.to_string_lossy().into_owned();
    unsafe { (api.string_free)(value) };
    Some(text)
}

pub fn native_language_hint(requested: &str, advertised: &[String]) -> Option<String> {
    let requested = requested.trim();
    if requested.is_empty() || requested.eq_ignore_ascii_case("auto") {
        return None;
    }
    let requested_base = requested.split(['-', '_']).next().unwrap_or(requested);
    advertised.iter().find(|language| language.eq_ignore_ascii_case(requested)).or_else(|| {
        advertised.iter().find(|language| language.split(['-', '_']).next().unwrap_or(language).eq_ignore_ascii_case(requested_base))
    }).cloned()
}

pub fn nemotron_live_uses_cpu(filename: &str) -> bool {
    cfg!(target_os = "macos") && filename.starts_with("nemotron-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    #[test]
    fn native_runtime_reports_pinned_version() {
        assert_eq!(native_runtime_version(), "0.2.3");
    }

    #[test]
    fn language_hint_uses_advertised_code() {
        let locales = ["fr-FR".to_string(), "en-US".to_string()];
        let bare = ["fr".to_string(), "en".to_string()];
        assert_eq!(native_language_hint("fr", &locales).as_deref(), Some("fr-FR"));
        assert_eq!(native_language_hint("fr", &bare).as_deref(), Some("fr"));
        assert_eq!(native_language_hint("auto", &locales), None);
        assert_eq!(native_language_hint("xx", &locales), None);
    }

    #[test]
    fn nemotron_live_requests_cpu_on_macos() {
        if cfg!(target_os = "macos") {
            assert!(nemotron_live_uses_cpu("nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf"));
            assert!(!nemotron_live_uses_cpu("Qwen3-ASR-0.6B-Q8_0.gguf"));
        }
    }

    #[test]
    fn nemotron_batch_returns_full_fixture_text() {
        let Some(root) = std::env::var_os("PARLEY_NATIVE_FIXTURE_DIR").map(PathBuf::from) else {
            eprintln!("PARLEY_NATIVE_FIXTURE_DIR unset, skipping real model batch");
            return;
        };
        let model = root.join("nemotron-3.5-asr-streaming-0.6b-Q8_0.gguf");
        let audio = root.join("french-s16.wav");
        assert!(model.is_file() && audio.is_file(), "native fixture missing in {}", root.display());
        let digest = format!("{:x}", Sha256::digest(std::fs::read(&model).unwrap()));
        assert_eq!(digest, "b94545b313b3223fda7b2857a52681da813935c2127643d1e9ff0c23d988089c");
        let mut reader = hound::WavReader::open(audio).unwrap();
        let spec = reader.spec();
        assert_eq!((spec.sample_rate, spec.channels, spec.bits_per_sample), (16_000, 1, 16));
        let pcm: Vec<f32> = reader.samples::<i16>().map(|sample| sample.unwrap() as f32 / 32768.0).collect();
        let text = transcribe_native_batch(&model, &pcm, None, false).unwrap();
        assert!(text.contains("Bonjour"), "{text}");
        assert!(text.split_whitespace().count() > 8, "{text}");
    }
}
