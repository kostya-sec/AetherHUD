//! Thin IPC surface for the webview. Every command only forwards to the window
//! controller: the shell owns the HUD, React never positions it.

use std::sync::Arc;

use serde::Deserialize;
use tauri::AppHandle;
use tauri::Manager;

use crate::window::geometry::LogicalRect;
use crate::window::{DismissReason, HudSnapshot, HudWindow, SummonReason};

/// Called by the webview once React mounted. This is the only point that maps the
/// window (off screen); later visibility is pure movement.
#[tauri::command]
pub fn frontend_ready(app: AppHandle) -> Result<HudSnapshot, String> {
    let hud = hud(&app)?;
    hud.mark_ready();
    Ok(hud.snapshot())
}

/// Esc in the HUD: slide away and hand focus back (#26).
#[tauri::command]
pub fn request_dismiss(app: AppHandle) -> Result<(), String> {
    let hud = hud(&app)?;
    hud.dismiss_from_user(DismissReason::Escape);
    Ok(())
}

/// Rectangles (CSS px, window relative) of every interactive control currently on screen.
/// The shell polls the cursor against them and flips whole-window click-through.
#[tauri::command]
pub fn set_hit_boxes(app: AppHandle, rects: Vec<CssRect>) -> Result<(), String> {
    let hud = hud(&app)?;
    hud.set_hit_boxes(
        rects
            .into_iter()
            .map(|rect| LogicalRect::new(rect.x, rect.y, rect.width, rect.height))
            .collect(),
    );
    Ok(())
}

/// Shell state for the settings layer (hotkey registration failures, launch mode).
#[tauri::command]
pub fn hud_status(app: AppHandle) -> Result<HudSnapshot, String> {
    Ok(hud(&app)?.snapshot())
}

/// Second launch without `--hidden`: summon the already running HUD (#12).
pub fn summon_existing(app: &AppHandle) {
    match app.try_state::<Arc<HudWindow>>() {
        Some(hud) => {
            let hud: Arc<HudWindow> = (*hud).clone();
            hud.summon_from_user(SummonReason::SecondInstance);
        }
        // The second instance arrived before setup finished: remember the request.
        None => crate::window::request_pending_summon(),
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CssRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

fn hud(app: &AppHandle) -> Result<Arc<HudWindow>, String> {
    app.try_state::<Arc<HudWindow>>()
        .map(|state| (*state).clone())
        .ok_or_else(|| "HUD 尚未初始化".to_string())
}
