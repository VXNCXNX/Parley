//! Detect the user's active foreground window so post-processing can adapt
//! its prompt to the app the user is dictating into (Glaido-style).

#[cfg(target_os = "windows")]
pub fn get_active_window_title() -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowTextW};

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.0.is_null() {
            return None;
        }
        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut buffer);
        if len <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buffer[..len as usize]);
        if title.trim().is_empty() {
            None
        } else {
            Some(title)
        }
    }
}

#[cfg(target_os = "macos")]
pub fn get_active_window_title() -> Option<String> {
    use log::{debug, warn};
    use std::process::Command;

    let output = Command::new("osascript")
        .args([
            "-e",
            "tell application \"System Events\" to set frontApp to first application process whose frontmost is true",
            "-e",
            "tell application \"System Events\" to set appName to name of frontApp",
            "-e",
            "tell application \"System Events\" to try",
            "-e",
            "set windowTitle to name of front window of frontApp",
            "-e",
            "on error",
            "-e",
            "set windowTitle to \"\"",
            "-e",
            "end try",
            "-e",
            "return appName & \" - \" & windowTitle",
        ])
        .output();

    let output = match output {
        Ok(output) => output,
        Err(e) => {
            warn!("Failed to query active macOS app via osascript: {}", e);
            return None;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!(
            "Failed to query active macOS app via osascript: {}",
            stderr.trim()
        );
        return None;
    }

    let title = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if title.is_empty() {
        None
    } else {
        debug!("Detected active macOS app/window: {}", title);
        Some(title)
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
pub fn get_active_window_title() -> Option<String> {
    None
}

/// Pick the best matching action key for the current active window title.
/// Returns the action_key from the first mapping whose pattern is a
/// case-insensitive substring of the title.
pub fn match_app_action(
    title: &str,
    mappings: &[crate::settings::AppPromptMapping],
) -> Option<u8> {
    let title_lower = title.to_lowercase();
    mappings
        .iter()
        .find(|m| {
            !m.pattern.trim().is_empty()
                && title_lower.contains(&m.pattern.to_lowercase())
        })
        .map(|m| m.action_key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::AppPromptMapping;

    #[test]
    fn matches_app_pattern_case_insensitively() {
        let mappings = vec![AppPromptMapping {
            pattern: "Codex".to_string(),
            action_key: 3,
        }];

        assert_eq!(match_app_action("codex - Parley", &mappings), Some(3));
    }

    #[test]
    fn ignores_empty_patterns() {
        let mappings = vec![
            AppPromptMapping {
                pattern: "".to_string(),
                action_key: 1,
            },
            AppPromptMapping {
                pattern: "ChatGPT".to_string(),
                action_key: 5,
            },
        ];

        assert_eq!(match_app_action("Codex", &mappings), None);
    }
}
