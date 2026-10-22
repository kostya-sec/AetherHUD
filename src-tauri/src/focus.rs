//! Handing keyboard focus back to the application that was foreground before the
//! HUD was summoned (#26 acceptance: 收起后原应用无需点击即可继续输入).
//!
//! The captured handle is reduced to a plain integer so the shell state stays
//! `Send + Sync` and can live in a `Mutex` across the animation, tray and hotkey
//! threads.

use tauri::{AppHandle, WebviewWindow};

/// Opaque reference to the previously foreground application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviousApp {
    raw: isize,
}

/// Remembers the current foreground application. Returns `None` when the HUD itself
/// is foreground (nothing to hand back) or when the platform cannot tell.
#[cfg(windows)]
pub fn capture(window: &WebviewWindow) -> Option<PreviousApp> {
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    // SAFETY: no arguments, no out pointers.
    let foreground = unsafe { GetForegroundWindow() };
    let raw = foreground.0 as isize;
    if raw == 0 {
        return None;
    }
    if let Ok(own) = window.hwnd() {
        if own.0 as isize == raw {
            return None;
        }
    }
    Some(PreviousApp { raw })
}

/// Restores the captured application as foreground. Returns false when the OS
/// refused (for example because the foreground lock was held elsewhere).
#[cfg(windows)]
pub fn restore(_app: &AppHandle, previous: PreviousApp) -> bool {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    // SAFETY: the handle was produced by GetForegroundWindow for this session; an
    // invalid handle only makes SetForegroundWindow fail.
    unsafe { SetForegroundWindow(HWND(previous.raw as *mut core::ffi::c_void)).as_bool() }
}

/// See the Windows overload above.
#[cfg(target_os = "macos")]
pub fn capture(window: &WebviewWindow) -> Option<PreviousApp> {
    use objc2_app_kit::NSWorkspace;

    let handle = window.app_handle().clone();
    // NSWorkspace must run on the main thread, but the caller may already be there
    // (tray clicks and the global hotkey arrive on the main thread on macOS).
    let pid = crate::platform::on_main_thread(&handle, || {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .map(|application| application.processIdentifier())
    })??;
    if pid <= 0 || pid == std::process::id() as i32 {
        return None;
    }
    Some(PreviousApp { raw: pid as isize })
}

/// See the Windows overload above.
#[cfg(target_os = "macos")]
pub fn restore(app: &AppHandle, previous: PreviousApp) -> bool {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

    crate::platform::on_main_thread(app, move || {
        let pid = previous.raw as i32;
        match NSRunningApplication::runningApplicationWithProcessIdentifier(pid) {
            Some(application) => {
                application.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows)
            }
            None => false,
        }
    })
    .unwrap_or(false)
}

/// Platforms without a focus-hand-back implementation.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn capture(_window: &WebviewWindow) -> Option<PreviousApp> {
    None
}

/// See the Windows overload above.
#[cfg(not(any(windows, target_os = "macos")))]
pub fn restore(_app: &AppHandle, _previous: PreviousApp) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::PreviousApp;

    #[test]
    fn previous_app_keeps_its_opaque_handle() {
        assert_eq!(PreviousApp { raw: 4711 }, PreviousApp { raw: 4711 });
        assert_ne!(PreviousApp { raw: 4711 }, PreviousApp { raw: 4712 });
    }
}
