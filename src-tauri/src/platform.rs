//! Small OS-specific helpers the shell needs but Tauri does not expose.

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
    use objc2_app_kit::{NSEvent, NSScreen};
    use objc2_foundation::MainThreadMarker;
    use tauri::Manager;

    // AppKit calls belong on the main thread; the supervisor polls off it.
    let (tx, rx) = std::sync::mpsc::channel();
    let handle = app.clone();
    handle
        .run_on_main_thread(move || {
            let value = (|| {
                let marker = MainThreadMarker::new()?;
                let screen = NSScreen::mainScreen(marker)?;
                let screen_height = screen.frame().size.height;
                let location = NSEvent::mouseLocation();
                Some((location.x, screen_height - location.y))
            })();
            let _ = tx.send(value);
        })
        .ok()?;
    let (x, y) = rx.recv().ok()??;
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
