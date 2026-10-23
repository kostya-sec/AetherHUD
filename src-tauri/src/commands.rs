//! Thin IPC surface for the webview. Every command only forwards to the window
//! controller: the shell owns the HUD, React never positions it.

use std::sync::Arc;

use aether_core::adapters::{Clock, SystemClock};
use aether_core::credentials::{
    CredentialKind, CredentialStatus, ValidationOutcome, ValidationTarget,
};
use aether_core::inbox::{InboxCursor, InboxPage};
use aether_core::ingest::IngestOutcome;
use aether_core::navigation::HudNavigation;
use aether_core::notice::HudNotice;
use aether_core::provider::ProviderProfile;
use serde::Deserialize;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;

use crate::events;

use crate::runtime::CoreState;
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
    // 点击穿透的命中区由前端在窗口上屏/布局变化后整体替换；debug 版留一行轨迹，
    // 便于真机排查「窗口隐藏时量到的矩形」（#29 回归）。
    if cfg!(debug_assertions) {
        let window = app.get_webview_window("main");
        eprintln!(
            "[aetherhud] hitboxes n={} first={:?} outer={:?} scale={:?}",
            rects.len(),
            rects.first(),
            window.as_ref().and_then(|w| w.outer_position().ok()),
            window.as_ref().and_then(|w| w.scale_factor().ok()),
        );
    }
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

/// 生效分类树派生的球列/L2 面板数据（#27）。核心库启动失败时返回可读错误，前端保留占位球。
#[tauri::command]
pub fn hud_navigation(state: tauri::State<'_, CoreState>) -> Result<HudNavigation, String> {
    state.navigation()
}

/// 取走启动期提示（覆盖层损坏等，5 秒提示条）；同一提示只返回一次。
#[tauri::command]
pub fn hud_take_notices(state: tauri::State<'_, CoreState>) -> Vec<HudNotice> {
    state.take_notices()
}

/// 本地录入（#28）：URL 进 fetch stage、纯文本直入 classify stage；成功后推
/// `hud://inbox-updated`，收件箱视图据此刷新。
#[tauri::command]
pub fn ingest_text(
    app: AppHandle,
    state: tauri::State<'_, CoreState>,
    text: String,
) -> Result<IngestOutcome, String> {
    let outcome = state.ingest_text(&text, SystemClock.now_ms())?;
    let _ = app.emit(events::INBOX_UPDATED, ());
    Ok(outcome)
}

/// 收件箱一页（50 条/页，created_at 倒序 keyset 分页；#28）。
#[tauri::command]
pub fn hud_inbox(
    state: tauri::State<'_, CoreState>,
    cursor: Option<InboxCursor>,
) -> Result<InboxPage, String> {
    state.inbox_page(cursor)
}

/// 活动 LLM 供应商配置（#45 前置）：预设 + base URL + 模型 + 消息格式。
/// 配置是非秘密 settings；Key 仍只存系统钥匙串。
#[tauri::command]
pub fn settings_provider_profile(
    state: tauri::State<'_, CoreState>,
) -> Result<ProviderProfile, String> {
    state.provider_profile()
}

/// 保存供应商配置（base URL / 模型 / 消息格式）；校验失败返回可读原因，成功后回读。
#[tauri::command]
pub fn settings_save_provider_profile(
    state: tauri::State<'_, CoreState>,
    profile: ProviderProfile,
) -> Result<ProviderProfile, String> {
    state.save_provider_profile(&profile)
}

/// 凭证区四行状态（已配置 / 未配置 / 钥匙串不可用）；只返回引用，永不返回密钥（#29）。
#[tauri::command]
pub fn settings_credential_statuses(
    state: tauri::State<'_, CoreState>,
) -> Result<Vec<CredentialStatus>, String> {
    state.credential_statuses()
}

/// 保存一项凭证：密钥写系统钥匙串，设置表只落 `keychain://` 引用（#29）。
#[tauri::command]
pub fn settings_save_credential(
    state: tauri::State<'_, CoreState>,
    kind: CredentialKind,
    secret: String,
) -> Result<CredentialStatus, String> {
    state.save_credential(kind, &secret)
}

/// 清除一项凭证（幂等）：删钥匙串密钥与设置引用（#29）。
#[tauri::command]
pub fn settings_clear_credential(
    state: tauri::State<'_, CoreState>,
    kind: CredentialKind,
) -> Result<CredentialStatus, String> {
    state.clear_credential(kind)
}

/// 验证凭证连通性：百炼（GET /models）、Jina（一次读）、飞书（tenant_access_token）。
/// 失败返回可读原因（代理 / Key / 权限），错误串已脱敏（#29）。
#[tauri::command]
pub async fn settings_validate_credential(
    state: tauri::State<'_, CoreState>,
    target: ValidationTarget,
) -> Result<ValidationOutcome, String> {
    state.validate_credential(target).await
}

/// 设置弹层底部「退出 AetherHUD」（#24 §10：退出只从托盘菜单与设置底部）。
#[tauri::command]
pub fn quit_app(app: AppHandle) {
    app.exit(0);
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
