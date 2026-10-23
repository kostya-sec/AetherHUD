//! Event names shared with the webview. Keep them in one place so the frontend
//! bridge (`src/platform/hudBridge.ts`) and the shell cannot drift apart.

/// The shell started summoning the HUD: reset to S0 and move focus into the search box.
pub const SUMMON: &str = "hud://summon";

/// The shell started sliding the HUD away: swallow every key until the next summon.
pub const DISMISS: &str = "hud://dismiss";

/// Tray menu "设置…": summon the HUD and open the settings layer (#29).
pub const OPEN_SETTINGS: &str = "hud://settings/open";

/// Tray menu "关于 / 检查更新": opens the settings layer's about section
/// (the update check itself lands with #47).
pub const OPEN_ABOUT: &str = "hud://about/open";

/// A new entry landed in the inbox (or a duplicate refreshed its last_seen_at).
/// The inbox list view reloads its first page; a closed inbox just marks itself stale.
pub const INBOX_UPDATED: &str = "hud://inbox-updated";
