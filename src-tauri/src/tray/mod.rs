//! Tray icon and menu (#24 §10 / #12).
//!
//! The tray is the fallback entry point: it exists even when the global hotkey could not
//! be registered. Left click toggles the HUD, right click opens the menu, and the toggle
//! item label mirrors the HUD phase.

use std::sync::Arc;

use tauri::menu::{MenuBuilder, MenuEvent, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::window::{HudWindow, Phase, UserTrigger};

/// Stable tray id; also used to update the tooltip.
pub const TRAY_ID: &str = "aetherhud-tray";

const MENU_TOGGLE: &str = "tray-toggle";
const MENU_SETTINGS: &str = "tray-settings";
const MENU_ABOUT: &str = "tray-about";
const MENU_QUIT: &str = "tray-quit";

/// Handle to the toggle menu item so its label can follow the HUD phase.
struct TrayMenu {
    toggle: MenuItem<tauri::Wry>,
}

/// Builds the tray icon. Returns an error only when the OS has no tray support; the
/// rest of the app must still start.
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, MENU_TOGGLE, "显示 HUD", true, None::<&str>)?;
    let menu = MenuBuilder::new(app)
        .item(&toggle)
        .text(MENU_SETTINGS, "设置…")
        .text(MENU_ABOUT, "关于 / 检查更新")
        .separator()
        .text(MENU_QUIT, "退出 AetherHUD")
        .build()?;

    let mut builder = TrayIconBuilder::with_id(TRAY_ID)
        .menu(&menu)
        // Left click toggles the HUD, right click opens the menu (#12). macOS builds
        // newer than 2.2 need this explicit flag, otherwise left click opens the menu.
        .show_menu_on_left_click(false)
        .tooltip(tooltip_for(Phase::Hidden))
        .on_menu_event(on_menu_event)
        .on_tray_icon_event(on_tray_event);

    #[cfg(target_os = "macos")]
    {
        // A template icon follows the menu bar's light/dark appearance.
        let icon =
            tauri::image::Image::from_bytes(include_bytes!("../../icons/tray-template.png"))?;
        builder = builder.icon(icon).icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(icon) = app.default_window_icon() {
            builder = builder.icon(icon.clone());
        } else {
            eprintln!("[aetherhud] the bundle has no window icon to reuse for the tray");
        }
    }

    builder.build(app)?;
    app.manage(TrayMenu { toggle });
    Ok(())
}

/// Keeps the menu label and tooltip in sync with the HUD phase. Status numbers
/// (分类中 / 待处理 / 离线挂起) arrive with the pipeline tickets; the phase lives here.
pub fn sync(app: &AppHandle, phase: Phase) {
    if let Some(menu) = app.try_state::<TrayMenu>() {
        let label = if phase.is_on_screen() {
            "收起 HUD"
        } else {
            "显示 HUD"
        };
        let _ = menu.toggle.set_text(label);
    }
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        let _ = tray.set_tooltip(Some(tooltip_for(phase)));
    }
}

fn tooltip_for(phase: Phase) -> &'static str {
    if phase.is_on_screen() {
        "AetherHUD · 显示中"
    } else {
        "AetherHUD · 已收起"
    }
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    let Some(hud) = app.try_state::<Arc<HudWindow>>() else {
        return;
    };
    match event.id().as_ref() {
        MENU_TOGGLE => hud.toggle_from_user(UserTrigger::Tray),
        MENU_SETTINGS => hud.open_settings(),
        MENU_ABOUT => hud.open_about(),
        // Quitting is only reachable from the tray menu and from the settings footer
        // (#12); app.exit runs ExitRequested/Exit so state can still be flushed later.
        MENU_QUIT => app.exit(0),
        _ => {}
    }
}

fn on_tray_event(tray: &TrayIcon, event: TrayIconEvent) {
    if let TrayIconEvent::Click {
        button: MouseButton::Left,
        button_state: MouseButtonState::Up,
        ..
    } = event
    {
        let app = tray.app_handle();
        if let Some(hud) = app.try_state::<Arc<HudWindow>>() {
            hud.toggle_from_user(UserTrigger::Tray);
        }
    }
}
