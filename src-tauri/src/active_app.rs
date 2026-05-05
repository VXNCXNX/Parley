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

#[cfg(not(target_os = "windows"))]
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
