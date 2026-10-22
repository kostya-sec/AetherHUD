//! macOS-only NSPanel integration (#24 §8).
//!
//! The HUD becomes a floating `NSPanel` so it can become key while floating above other
//! windows, and `window_did_resign_key` gives the shell a second blur channel next to
//! Tauri's `Focused(false)` event. Every failure here is non-fatal: the controller falls
//! back to the plain transparent window, which still summons and dismisses.
//!
//! The panel handle is kept in app state instead of relying on the plugin's registry:
//! the shell only ever has one panel, and this keeps the setup order obvious.

use std::sync::Arc;

use tauri::{AppHandle, Manager, WebviewWindow, Wry};
use tauri_nspanel::{tauri_panel, Panel, PanelLevel};

use super::HudWindow;

// The macro emits the objc2 / AppKit imports it needs at this call site, so this
// module only imports what its own code names.
tauri_panel! {
    panel!(HudPanel {
        config: {
            can_become_key_window: true,
            can_become_main_window: false,
            is_floating_panel: true,
            hides_on_deactivate: false
        }
    })

    panel_event!(HudPanelEvents {
        window_did_resign_key(notification: &NSNotification) -> ()
    })
}

/// The one HUD panel, kept in app state so summon/dismiss can make it key or resign it.
struct HudPanelState(HudPanel);

/// Converts the main window into the HUD panel and wires the blur channel.
pub fn install(window: &WebviewWindow, hud: Arc<HudWindow>) -> tauri::Result<()> {
    let panel = HudPanel::<Wry>::from_window(window)?;
    panel.set_level(PanelLevel::Floating.value());
    panel.set_floating_panel(true);
    panel.set_hides_on_deactivate(false);
    panel.set_becomes_key_only_if_needed(false);

    let events = HudPanelEvents::new();
    events.window_did_resign_key(move |_notification| {
        // Same grace-window path as Tauri's Focused(false): the supervisor decides.
        hud.on_focus_changed(false);
    });
    panel.set_event_handler(Some(events.as_ref()));

    window.app_handle().manage(HudPanelState(panel));
    Ok(())
}

/// Makes the panel key when the HUD is summoned.
pub fn make_key(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(panel) = handle.try_state::<HudPanelState>() {
            panel.0.make_key_window();
        }
    });
}

/// Gives key status back when the HUD starts dismissing.
pub fn resign_key(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(panel) = handle.try_state::<HudPanelState>() {
            panel.0.resign_key_window();
        }
    });
}
