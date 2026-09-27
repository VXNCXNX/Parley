use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::path::Path;
use transcribe_cpp::{Backend, Model, ModelOptions, RunOptions};

fn backend_for(path: &Path, live: bool) -> Backend {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if live && cfg!(target_os = "macos") && name.starts_with("nemotron-") {
        return Backend::Cpu;
    }
    if cfg!(target_os = "macos") {
        Backend::Metal
    } else {
        Backend::Auto
    }
}

fn advertised_language(requested: Option<&str>, available: &[String]) -> Option<String> {
    let requested = requested?.trim();
    if requested.is_empty() || requested.eq_ignore_ascii_case("auto") {
        return None;
    }
    if let Some(exact) = available
        .iter()
        .find(|language| language.eq_ignore_ascii_case(requested))
    {
        return Some(exact.clone());
    }
    let requested_base = requested.split(['-', '_']).next().unwrap_or(requested);
    available
        .iter()
        .find(|language| {
            language
                .split(['-', '_'])
                .next()
                .unwrap_or(language)
                .eq_ignore_ascii_case(requested_base)
        })
        .cloned()
}

fn run_language(arch: &str, requested: Option<&str>, available: &[String]) -> Option<String> {
    if arch == "qwen3_asr" {
        return None;
    }
    advertised_language(requested, available)
}

fn set_error(error_out: *mut *mut c_char, message: String) -> c_int {
    if !error_out.is_null() {
        let owned =
            CString::new(message).unwrap_or_else(|_| CString::new("native runtime error").unwrap());
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
        return set_error(
            error_out,
            format!(
                "transcribe-cpp linked {}, expected 0.2.3",
                transcribe_cpp::version()
            ),
        );
    }
    if let Err(error) = transcribe_cpp::init_backends_default() {
        return set_error(error_out, error.to_string());
    }
    let model = match Model::load_with(
        path,
        &ModelOptions {
            backend: backend_for(path, live_backend != 0),
            ..Default::default()
        },
    ) {
        Ok(model) => model,
        Err(error) => return set_error(error_out, error.to_string()),
    };
    let mut session = match model.session() {
        Ok(session) => session,
        Err(error) => return set_error(error_out, error.to_string()),
    };
    let requested_language = if language.is_null() {
        None
    } else {
        match CStr::from_ptr(language).to_str() {
            Ok(language) => Some(language),
            Err(_) => return set_error(error_out, "language is not utf-8".to_string()),
        }
    };
    let mut options = RunOptions::default();
    options.language = run_language(
        &model.arch(),
        requested_language,
        &model.capabilities().languages,
    );
    let audio = std::slice::from_raw_parts(samples, sample_count);
    match session.run(audio, &options) {
        Ok(transcript) => match CString::new(transcript.text) {
            Ok(text) => {
                *text_out = text.into_raw();
                0
            }
            Err(_) => set_error(
                error_out,
                "native transcript contains an interior nul".to_string(),
            ),
        },
        Err(error) => set_error(error_out, error.to_string()),
    }
}

#[no_mangle]
pub unsafe extern "C" fn parley_native_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(CString::from_raw(value));
    }
}

use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};

enum WorkerCmd {
    Feed(Vec<f32>, Sender<Result<Option<(String, String)>, String>>),
    Finalize(Sender<Result<String, String>>),
    Cancel,
}

struct NativeWorker {
    tx: Sender<WorkerCmd>,
    handle: Option<JoinHandle<()>>,
}

fn start_worker(path: &Path, language: Option<String>, live: bool) -> Result<NativeWorker, String> {
    let path = path.to_path_buf();
    let (tx, rx) = mpsc::channel();
    let handle = thread::spawn(move || worker_loop(path, language, live, rx));
    Ok(NativeWorker {
        tx,
        handle: Some(handle),
    })
}

fn worker_loop(
    path: std::path::PathBuf,
    language: Option<String>,
    live: bool,
    rx: Receiver<WorkerCmd>,
) {
    let loaded = (|| -> Result<(Model, bool), String> {
        transcribe_cpp::init_backends_default().map_err(|error| error.to_string())?;
        let model = Model::load_with(
            &path,
            &ModelOptions {
                backend: backend_for(&path, live),
                ..Default::default()
            },
        )
        .map_err(|error| error.to_string())?;
        let streaming = live && model.capabilities().supports_streaming;
        Ok((model, streaming))
    })();
    let (model, streaming) = match loaded {
        Ok(loaded) => loaded,
        Err(error) => {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    WorkerCmd::Feed(_, reply) => {
                        let _ = reply.send(Err(error.clone()));
                    }
                    WorkerCmd::Finalize(reply) => {
                        let _ = reply.send(Err(error.clone()));
                    }
                    WorkerCmd::Cancel => break,
                }
            }
            return;
        }
    };
    let language = run_language(
        &model.arch(),
        language.as_deref(),
        &model.capabilities().languages,
    );
    run_worker(model, language, streaming, rx);
}

fn run_worker(model: Model, language: Option<String>, streaming: bool, rx: Receiver<WorkerCmd>) {
    let mut session = match model.session() {
        Ok(session) => session,
        Err(error) => {
            while let Ok(cmd) = rx.recv() {
                match cmd {
                    WorkerCmd::Feed(_, reply) => {
                        let _ = reply.send(Err(error.to_string()));
                    }
                    WorkerCmd::Finalize(reply) => {
                        let _ = reply.send(Err(error.to_string()));
                    }
                    WorkerCmd::Cancel => break,
                }
            }
            return;
        }
    };
    let mut options = RunOptions::default();
    options.language = language;
    if streaming {
        let mut stream = match session.stream(&options, &transcribe_cpp::StreamOptions::default()) {
            Ok(stream) => stream,
            Err(error) => {
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        WorkerCmd::Feed(_, reply) => {
                            let _ = reply.send(Err(error.to_string()));
                        }
                        WorkerCmd::Finalize(reply) => {
                            let _ = reply.send(Err(error.to_string()));
                        }
                        WorkerCmd::Cancel => break,
                    }
                }
                return;
            }
        };
        while let Ok(cmd) = rx.recv() {
            match cmd {
                WorkerCmd::Feed(frame, reply) => {
                    let result = stream
                        .feed(&frame)
                        .map(|update| {
                            if !(update.committed_changed || update.tentative_changed) {
                                return None;
                            }
                            let text = stream.text();
                            Some((text.committed, text.tentative))
                        })
                        .map_err(|error| error.to_string());
                    let _ = reply.send(result);
                }
                WorkerCmd::Finalize(reply) => {
                    let result = stream
                        .finalize()
                        .map(|_| stream.text().full.clone())
                        .map_err(|error| error.to_string());
                    let _ = reply.send(result);
                    break;
                }
                WorkerCmd::Cancel => break,
            }
        }
        return;
    }
    let mut audio = Vec::<f32>::new();
    while let Ok(cmd) = rx.recv() {
        match cmd {
            WorkerCmd::Feed(frame, reply) => {
                audio.extend_from_slice(&frame);
                let _ = reply.send(Ok(None));
            }
            WorkerCmd::Finalize(reply) => {
                let result = session
                    .run(&audio, &options)
                    .map(|transcript| transcript.text)
                    .map_err(|error| error.to_string());
                let _ = reply.send(result);
                break;
            }
            WorkerCmd::Cancel => break,
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn parley_native_worker_begin(
    path: *const c_char,
    language: *const c_char,
    live: c_int,
    worker_out: *mut *mut NativeWorker,
    error_out: *mut *mut c_char,
) -> c_int {
    if path.is_null() || worker_out.is_null() {
        return set_error(error_out, "missing native worker input".to_string());
    }
    let path = match CStr::from_ptr(path).to_str() {
        Ok(path) => Path::new(path),
        Err(_) => return set_error(error_out, "model path is not utf-8".to_string()),
    };
    let language = if language.is_null() {
        None
    } else {
        match CStr::from_ptr(language).to_str() {
            Ok(language) if !language.is_empty() => Some(language.to_string()),
            Ok(_) => None,
            Err(_) => return set_error(error_out, "language is not utf-8".to_string()),
        }
    };
    match start_worker(path, language, live != 0) {
        Ok(worker) => {
            *worker_out = Box::into_raw(Box::new(worker));
            0
        }
        Err(error) => set_error(error_out, error),
    }
}

#[no_mangle]
pub unsafe extern "C" fn parley_native_worker_feed(
    worker: *mut NativeWorker,
    samples: *const f32,
    sample_count: usize,
    committed_out: *mut *mut c_char,
    tentative_out: *mut *mut c_char,
    error_out: *mut *mut c_char,
) -> c_int {
    if worker.is_null() || samples.is_null() {
        return set_error(error_out, "missing native feed input".to_string());
    }
    let audio = std::slice::from_raw_parts(samples, sample_count).to_vec();
    let (reply_tx, reply_rx) = mpsc::channel();
    if (*worker).tx.send(WorkerCmd::Feed(audio, reply_tx)).is_err() {
        return set_error(error_out, "native worker stopped".to_string());
    }
    match reply_rx.recv() {
        Ok(Ok(Some((committed, tentative)))) => {
            if !committed_out.is_null() {
                *committed_out = CString::new(committed)
                    .unwrap_or_else(|_| CString::new("").unwrap())
                    .into_raw();
            }
            if !tentative_out.is_null() {
                *tentative_out = CString::new(tentative)
                    .unwrap_or_else(|_| CString::new("").unwrap())
                    .into_raw();
            }
            0
        }
        Ok(Ok(None)) => 0,
        Ok(Err(error)) => set_error(error_out, error),
        Err(_) => set_error(error_out, "native worker stopped during feed".to_string()),
    }
}

#[no_mangle]
pub unsafe extern "C" fn parley_native_worker_finalize(
    worker: *mut NativeWorker,
    text_out: *mut *mut c_char,
    error_out: *mut *mut c_char,
) -> c_int {
    if worker.is_null() || text_out.is_null() {
        return set_error(error_out, "missing native finalize input".to_string());
    }
    let (reply_tx, reply_rx) = mpsc::channel();
    if (*worker).tx.send(WorkerCmd::Finalize(reply_tx)).is_err() {
        return set_error(error_out, "native worker stopped".to_string());
    }
    match reply_rx.recv() {
        Ok(Ok(text)) => {
            *text_out = CString::new(text)
                .unwrap_or_else(|_| CString::new("").unwrap())
                .into_raw();
            0
        }
        Ok(Err(error)) => set_error(error_out, error),
        Err(_) => set_error(
            error_out,
            "native worker stopped during finalize".to_string(),
        ),
    }
}

#[no_mangle]
pub unsafe extern "C" fn parley_native_worker_cancel(worker: *mut NativeWorker) {
    if worker.is_null() {
        return;
    }
    let worker = Box::from_raw(worker);
    let _ = worker.tx.send(WorkerCmd::Cancel);
    if let Some(handle) = worker.handle {
        let _ = handle.join();
    }
}

#[cfg(test)]
mod tests {
    use super::{advertised_language, run_language};

    #[test]
    fn maps_requested_language_to_model_locale() {
        let nemotron = vec!["en-US".to_string(), "fr-FR".to_string()];
        let qwen = vec!["en".to_string(), "fr".to_string()];
        assert_eq!(
            advertised_language(Some("fr"), &nemotron).as_deref(),
            Some("fr-FR")
        );
        assert_eq!(
            advertised_language(Some("fr"), &qwen).as_deref(),
            Some("fr")
        );
        assert_eq!(advertised_language(Some("auto"), &nemotron), None);
        assert_eq!(advertised_language(Some("xx"), &nemotron), None);
        assert_eq!(run_language("qwen3_asr", Some("fr"), &qwen), None);
    }
}
