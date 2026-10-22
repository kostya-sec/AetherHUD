//! Thin OS shell only: the transparent HUD window and the IPC seam the frontend
//! uses to reveal it after the first paint. Business behavior belongs to the
//! future aether-core library, not here.

/// Reveals the window after the webview painted its first frame. The window is
/// created hidden so a transparent window never flashes white on startup.
#[tauri::command]
fn frontend_ready(window: tauri::WebviewWindow) -> Result<(), String> {
    window.show().map_err(|error| error.to_string())?;
    window.set_focus().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![frontend_ready])
        .run(tauri::generate_context!())
        .expect("error while running AetherHUD");
}
