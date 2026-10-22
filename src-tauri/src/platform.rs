//! Small OS-specific helpers the shell needs but Tauri does not expose.

/// Runs `task` on the macOS main thread and returns its result, or runs it inline when
/// the caller already is the main thread.
///
/// `run_on_main_thread` only *queues* the task, so waiting for it from the main thread
/// would deadlock. Tray clicks, the global hotkey and window events all arrive on the
/// main thread, which is exactly where `start_summon` captures the previous application.
#[cfg(target_os = "macos")]
pub fn on_main_thread<T, F>(app: &tauri::AppHandle, task: F) -> Option<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    use tauri::Manager;

    if objc2::MainThreadMarker::new().is_some() {
        return Some(task());
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let app = app.clone();
    app.run_on_main_thread(move || {
        let _ = tx.send(task());
    })
    .ok()?;
    rx.recv().ok()
}

/// Current cursor position in physical screen pixels.
///
/// Windows reports physical pixels directly (including negative origins on
/// multi-monitor desktops). macOS reports points with a bottom-left origin of the
/// primary screen, so the value is flipped and scaled into the same space winit uses
/// for window positions. The macOS path is best effort under mixed-DPI setups until
/// it can be validated on a real Mac (#26 manual regression).
#[cfg(windows)]
pub fn cursor_position(_app: &tauri::AppHandle, _scale_factor: f64) -> Option<(i32, i32)> {
    use windows::Win32::Foundation::POINT;
    use windows::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: GetCursorPos only writes into the POINT we own.
    unsafe {
        GetCursorPos(&mut point).ok()?;
    }
    Some((point.x, point.y))
}

/// See the Windows overload above.
#[cfg(target_os = "macos")]
pub fn cursor_position(app: &tauri::AppHandle, scale_factor: f64) -> Option<(i32, i32)> {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSEvent, NSScreen};

    let (x, y) = on_main_thread(app, || {
        let marker = MainThreadMarker::new()?;
        let screen = NSScreen::mainScreen(marker)?;
        let screen_height = screen.frame().size.height;
        let location = NSEvent::mouseLocation();
        Some((location.x, screen_height - location.y))
    })??;

    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    Some(((x * scale).round() as i32, (y * scale).round() as i32))
}

/// See the Windows overload above.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn cursor_position(_app: &tauri::AppHandle, _scale_factor: f64) -> Option<(i32, i32)> {
    None
}
