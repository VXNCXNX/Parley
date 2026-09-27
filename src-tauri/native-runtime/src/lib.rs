use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use std::ptr;
use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions};

fn backend_for(path: &Path, live: bool) -> Backend {
    let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    if live && cfg!(target_os = "macos") && name.starts_with("nemotron-") {
        return Backend::Cpu;
    }
    if cfg!(target_os = "macos") { Backend::Metal } else { Backend::Auto }
}

fn set_error(error_out: *mut *mut c_char, message: String) -> c_int {
    if !error_out.is_null() {
        let owned = CString::new(message).unwrap_or_else(|_| CString::new("native runtime error").unwrap());
        unsafe { *error_out = owned.into_raw() };
    }
    1
}

#[no_mangle]
pub unsafe extern "C" fn parley_native_transcribe_batch(
    path: *const c_char,
    samples: *const f32,
    sample_count: usize,
    language: *const c_char,
    live_backend: c_int,
    text_out: *mut *mut c_char,
    error_out: *mut *mut c_char,
) -> c_int {
    if path.is_null() || samples.is_null() || text_out.is_null() {
        return set_error(error_out, "missing native transcription input".to_string());
    }
    let path = match CStr::from_ptr(path).to_str() {
        Ok(path) => Path::new(path),
        Err(_) => return set_error(error_out, "model path is not utf-8".to_string()),
    };
    if transcribe_cpp::version() != "0.2.3" {
        return set_error(error_out, format!("transcribe-cpp linked {}, expected 0.2.3", transcribe_cpp::version()));
    }
    if let Err(error) = transcribe_cpp::init_backends_default() {
        return set_error(error_out, error.to_string());
    }
    let model = match Model::load_with(path, &ModelOptions { backend: backend_for(path, live_backend != 0), ..Default::default() }) {
        Ok(model) => model,
        Err(error) => return set_error(error_out, error.to_string()),
    };
    let mut session = match model.session() {
        Ok(session) => session,
        Err(error) => return set_error(error_out, error.to_string()),
    };
    let mut options = RunOptions::default();
    if !language.is_null() {
        match CStr::from_ptr(language).to_str() {
            Ok(language) if !language.is_empty() => options.language = Some(language.to_string()),
            Ok(_) => {}
            Err(_) => return set_error(error_out, "language is not utf-8".to_string()),
        }
    }
    let audio = std::slice::from_raw_parts(samples, sample_count);
    match session.run(audio, &options) {
        Ok(transcript) => {
            match CString::new(transcript.text) {
                Ok(text) => {
                    *text_out = text.into_raw();
                    0
                }
                Err(_) => set_error(error_out, "native transcript contains an interior nul".to_string()),
            }
        }
        Err(error) => set_error(error_out, error.to_string()),
    }
}

#[no_mangle]
pub unsafe extern "C" fn parley_native_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(CString::from_raw(value));
    }
}

#[allow(dead_code)]
fn keep_ptr_import() { let _ = ptr::null::<c_char>(); }
