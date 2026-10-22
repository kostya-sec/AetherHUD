//! Thin OS shell only: the transparent HUD window and the IPC seam the frontend
//! uses to reveal it after the first paint. Business behavior belongs to the
//! future aether-core library, not here.

use std::time::Duration;
use tauri::Manager;

/// Reveals and focuses the hidden HUD window.
///
/// This is the #4-sanctioned initial `visible:false` + `show()` path, not the HUD
/// dismiss state machine: off-screen docking + `set_position` for summon/dismiss
/// belongs to T02. There is no position change to apply on initial reveal.
fn reveal(window: &tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

/// Called by the webview once React has mounted.
#[tauri::command]
fn frontend_ready(window: tauri::WebviewWindow) -> Result<(), String> {
    reveal(&window)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Safety net: a hidden WebView must never become an invisible zombie
            // if the frontend fails to load or its IPC never arrives.
            if let Some(window) = app.get_webview_window("main") {
                let fallback = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(Duration::from_secs(3));
                    if !fallback.is_visible().unwrap_or(false) {
                        let _ = reveal(&fallback);
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![frontend_ready])
        .run(tauri::generate_context!())
        .expect("error while running AetherHUD");
}
