//! Thin OS shell only: the resident transparent HUD window, its tray entry, the global
//! hotkey and the IPC seam the webview uses. Business behavior belongs to
//! `aether-core`, not here.

mod commands;
mod events;
mod focus;
mod hotkey;
mod platform;
mod runtime;
mod startup;
mod tray;
mod window;

use std::sync::Arc;

use tauri::Manager;

use startup::{parse_launch_mode, LaunchMode};
use window::HudWindow;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let launch_mode = parse_launch_mode(std::env::args());

    // #12: single-instance must be registered before every other plugin so a second
    // process is detected and handed over before any setup code runs.
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // `--hidden` means the login item fired again: stay resident, do not summon.
            if parse_launch_mode(argv) == LaunchMode::Resident {
                return;
            }
            commands::summon_existing(app);
        }))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build());

    builder
        .setup(move |app| {
            setup(app, launch_mode)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::frontend_ready,
            commands::request_dismiss,
            commands::set_hit_boxes,
            commands::hud_status,
            commands::hud_navigation,
            commands::hud_take_notices,
        ])
        .run(tauri::generate_context!())
        .expect("error while running AetherHUD");
}

fn setup(app: &mut tauri::App, launch_mode: LaunchMode) -> Result<(), Box<dyn std::error::Error>> {
    let window = app
        .get_webview_window("main")
        .ok_or("the main HUD window is missing from tauri.conf.json")?;

    // macOS: pure menu-bar utility, no Dock icon (#12). The "show in Dock" setting in
    // the general settings section flips this policy at runtime (T22).
    #[cfg(target_os = "macos")]
    app.set_activation_policy(tauri::ActivationPolicy::Accessory);

    let hud = HudWindow::attach(app.handle().clone(), window, launch_mode);
    app.manage(Arc::clone(&hud));

    // 领域核心（分类树合并、meta.taxonomy_version、HUD 导航数据）。失败不阻塞壳：
    // 球列退回占位，命令返回可读错误（#27 验收：启动 fail-safe）。
    app.manage(runtime::CoreState::bootstrap(app.handle()));

    // macOS: convert to an NSPanel and add the window_did_resign_key blur channel.
    // A failure is not fatal; the plain transparent window still works.
    #[cfg(target_os = "macos")]
    if let Err(error) = window::install_panel(&hud) {
        eprintln!("[aetherhud] NSPanel install failed, falling back to the plain window: {error}");
    }

    // Hotkey registration must never break startup (#26): failures are captured into
    // HotkeyState, logged, and the tray stays as the fallback path. Register before the
    // tray so its tooltip can show the accelerator.
    app.manage(hotkey::HotkeyState::new(hotkey::register(app.handle())));

    tray::build(app.handle())?;

    window::spawn_supervisor(Arc::clone(&hud));
    window::spawn_reveal_fallback(Arc::clone(&hud));

    Ok(())
}
