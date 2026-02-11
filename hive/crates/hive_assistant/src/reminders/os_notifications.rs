/// Show a native OS toast notification.
///
/// On Windows, uses `winrt-notification` to display a toast in the
/// notification center. On other platforms this is a no-op (actual
/// notification support to be added per-platform as needed).
pub fn show_toast(title: &str, body: &str) {
    #[cfg(windows)]
    {
        show_toast_windows(title, body);
    }
    #[cfg(not(windows))]
    {
        let _ = (title, body);
        tracing::debug!("OS notifications not implemented for this platform");
    }
}

#[cfg(windows)]
fn show_toast_windows(title: &str, body: &str) {
    use winrt_notification::{Duration, Toast};

    let result = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(body)
        .duration(Duration::Short)
        .show();

    if let Err(e) = result {
        tracing::warn!("Failed to show Windows toast notification: {e}");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::reminders::os_notifications::show_toast;

    #[test]
    fn test_show_toast_does_not_panic() {
        // This test just verifies the function can be called without panicking.
        // On CI or headless environments, the toast may silently fail, which is fine.
        show_toast("Test Title", "Test Body");
    }

    #[test]
    fn test_show_toast_empty_strings() {
        show_toast("", "");
    }

    #[test]
    fn test_show_toast_unicode() {
        show_toast("Reminder", "Meeting at 3pm ");
    }
}
