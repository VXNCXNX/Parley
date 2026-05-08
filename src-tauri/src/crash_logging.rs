use chrono::Local;
use std::backtrace::Backtrace;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::panic::{self, PanicHookInfo};
use std::path::{Path, PathBuf};
use std::sync::Once;
use tauri::{AppHandle, Manager};

static PANIC_HOOK: Once = Once::new();

pub fn install_panic_logging(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let log_dir = app_handle
        .path()
        .app_log_dir()
        .map_err(|err| format!("Failed to resolve app log dir: {err}"))?;

    fs::create_dir_all(&log_dir).map_err(|err| {
        format!(
            "Failed to create app log dir '{}': {err}",
            log_dir.display()
        )
    })?;

    let crash_log_path = log_dir.join("parley-crash.log");

    PANIC_HOOK.call_once({
        let crash_log_path = crash_log_path.clone();
        move || {
            let previous_hook = panic::take_hook();
            panic::set_hook(Box::new(move |panic_info| {
                if let Err(err) = append_panic_report(&crash_log_path, panic_info) {
                    eprintln!(
                        "Failed to append panic report to '{}': {}",
                        crash_log_path.display(),
                        err
                    );
                }

                previous_hook(panic_info);
            }));
        }
    });

    Ok(crash_log_path)
}

fn append_panic_report(path: &Path, panic_info: &PanicHookInfo<'_>) -> Result<(), String> {
    let timestamp = Local::now().to_rfc3339();
    let thread = std::thread::current();
    let thread_name = thread.name().unwrap_or("unnamed");
    let location = panic_info
        .location()
        .map(|location| format!("{}:{}", location.file(), location.line()))
        .unwrap_or_else(|| "unknown".to_string());
    let payload = extract_panic_payload(panic_info);
    let backtrace = Backtrace::force_capture();

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("Failed to open crash log '{}': {err}", path.display()))?;

    writeln!(
        file,
        "[{timestamp}] panic on thread '{thread_name}' at {location}\nmessage: {payload}\nbacktrace:\n{backtrace}\n"
    )
    .map_err(|err| format!("Failed to write crash log '{}': {err}", path.display()))
}

fn extract_panic_payload(panic_info: &PanicHookInfo<'_>) -> String {
    if let Some(message) = panic_info.payload().downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = panic_info.payload().downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}
