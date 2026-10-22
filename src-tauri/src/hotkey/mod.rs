//! Global hotkey for summon/dismiss (#24 §10, #12).
//!
//! Registration must never break startup: a conflicting accelerator is logged, surfaced
//! to the settings layer through `HotkeyStatus`, and the tray icon keeps working as the
//! fallback summon path.

use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use crate::window::{HudWindow, UserTrigger};

/// Windows default. Backquote avoids the `Alt+Space` system-window-menu conflict.
pub const WINDOWS_DEFAULT: &str = "Alt+`";

/// macOS default: Option+Space (^Space and Cmd+Space belong to the system).
#[cfg(target_os = "macos")]
pub const MACOS_DEFAULT: &str = "Option+Space";

/// The platform default accelerator.
pub fn default_accelerator() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        MACOS_DEFAULT
    }
    #[cfg(not(target_os = "macos"))]
    {
        WINDOWS_DEFAULT
    }
}

/// How modifier keys are spelled in user-facing hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierStyle {
    /// "Alt + `", "Ctrl + Shift + Space" (Windows).
    Words,
    /// "⌥`", "⌃⇧Space" (macOS).
    Symbols,
}

impl ModifierStyle {
    /// The style of the platform this binary runs on.
    pub fn native() -> Self {
        if cfg!(target_os = "macos") {
            ModifierStyle::Symbols
        } else {
            ModifierStyle::Words
        }
    }
}

/// Human-readable accelerator for tooltips, the tray and the settings layer.
pub fn display_accelerator(accelerator: &str) -> String {
    display_with(accelerator, ModifierStyle::native())
}

/// Human-readable accelerator with an explicit modifier style (also used by tests).
pub fn display_with(accelerator: &str, style: ModifierStyle) -> String {
    let parts: Vec<String> = accelerator
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(|part| display_token(part, style))
        .collect();

    match style {
        ModifierStyle::Words => parts.join(" + "),
        ModifierStyle::Symbols => parts.concat(),
    }
}

fn display_token(token: &str, style: ModifierStyle) -> String {
    let upper = token.to_uppercase();
    match (upper.as_str(), style) {
        ("OPTION" | "ALT", ModifierStyle::Words) => "Alt".to_string(),
        ("OPTION" | "ALT", ModifierStyle::Symbols) => "⌥".to_string(),
        ("CONTROL" | "CTRL", ModifierStyle::Words) => "Ctrl".to_string(),
        ("CONTROL" | "CTRL", ModifierStyle::Symbols) => "⌃".to_string(),
        ("COMMAND" | "CMD" | "SUPER", ModifierStyle::Words) => "Win".to_string(),
        ("COMMAND" | "CMD" | "SUPER", ModifierStyle::Symbols) => "⌘".to_string(),
        ("SHIFT", ModifierStyle::Words) => "Shift".to_string(),
        ("SHIFT", ModifierStyle::Symbols) => "⇧".to_string(),
        (
            "COMMANDORCONTROL" | "COMMANDORCTRL" | "CMDORCTRL" | "CMDORCONTROL",
            ModifierStyle::Words,
        ) => "Ctrl".to_string(),
        (
            "COMMANDORCONTROL" | "COMMANDORCTRL" | "CMDORCTRL" | "CMDORCONTROL",
            ModifierStyle::Symbols,
        ) => "⌘".to_string(),
        _ => display_key(token),
    }
}

fn display_key(key: &str) -> String {
    let upper = key.to_uppercase();
    match upper.as_str() {
        "BACKQUOTE" | "`" => "`".to_string(),
        "SPACE" => "Space".to_string(),
        "ENTER" | "RETURN" => "Enter".to_string(),
        "TAB" => "Tab".to_string(),
        "ESCAPE" | "ESC" => "Esc".to_string(),
        "BACKSPACE" => "Backspace".to_string(),
        "DELETE" => "Del".to_string(),
        "ARROWUP" | "UP" => "↑".to_string(),
        "ARROWDOWN" | "DOWN" => "↓".to_string(),
        "ARROWLEFT" | "LEFT" => "←".to_string(),
        "ARROWRIGHT" | "RIGHT" => "→".to_string(),
        other => {
            if let Some(digit) = other.strip_prefix("DIGIT") {
                return digit.to_string();
            }
            if let Some(letter) = other.strip_prefix("KEY") {
                if letter.len() == 1 {
                    return letter.to_string();
                }
            }
            key.to_string()
        }
    }
}

/// Validates an accelerator and returns its canonical form (e.g. `alt+Backquote`).
/// Used by the settings layer before rebinding and at startup before registering.
pub fn canonical_accelerator(accelerator: &str) -> Result<String, String> {
    accelerator
        .parse::<Shortcut>()
        .map(|shortcut| shortcut.into_string())
        .map_err(|error| error.to_string())
}

/// User-facing Chinese message for the settings layer (#22) and the log.
pub fn registration_failure_message(accelerator: &str, error: &str) -> String {
    format!(
        "热键 {} 注册失败，可能已被其他应用占用（{}）。HUD 仍可从托盘图标唤出，稍后可在设置中重新绑定。",
        display_accelerator(accelerator),
        error
    )
}

/// Registration state exposed to the frontend through `hud_status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HotkeyStatus {
    /// Accelerator as configured ("Alt+`"), not the canonical form.
    pub accelerator: String,
    /// User-facing label ("Alt + `").
    pub display: String,
    /// True when the OS accepted the registration.
    pub registered: bool,
    /// Registration failure, if any.
    pub error: Option<String>,
}

impl HotkeyStatus {
    pub fn registered(accelerator: &str) -> Self {
        Self {
            accelerator: accelerator.to_string(),
            display: display_accelerator(accelerator),
            registered: true,
            error: None,
        }
    }

    pub fn failed(accelerator: &str, error: impl Into<String>) -> Self {
        Self {
            accelerator: accelerator.to_string(),
            display: display_accelerator(accelerator),
            registered: false,
            error: Some(error.into()),
        }
    }
}

/// Registration result kept in app state for the settings layer.
#[derive(Debug)]
pub struct HotkeyState(Mutex<HotkeyStatus>);

impl HotkeyState {
    pub fn new(status: HotkeyStatus) -> Self {
        Self(Mutex::new(status))
    }

    pub fn status(&self) -> HotkeyStatus {
        self.0
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .clone()
    }
}

/// Registers the summon/dismiss hotkey. Every failure is contained: the app keeps
/// running and the tray remains the fallback entry point.
pub fn register<R: Runtime>(app: &AppHandle<R>) -> HotkeyStatus {
    let accelerator = default_accelerator();

    if let Err(error) = canonical_accelerator(accelerator) {
        let status = HotkeyStatus::failed(accelerator, error);
        log_failure(&status);
        return status;
    }

    let result = app.global_shortcut().on_shortcut(
        accelerator,
        move |app: &AppHandle<R>, _shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }
            let Some(window) = app.try_state::<std::sync::Arc<HudWindow>>() else {
                return;
            };
            window.toggle_from_user(UserTrigger::Hotkey);
        },
    );

    match result {
        Ok(()) => HotkeyStatus::registered(accelerator),
        Err(error) => {
            let status = HotkeyStatus::failed(accelerator, error.to_string());
            log_failure(&status);
            status
        }
    }
}

fn log_failure(status: &HotkeyStatus) {
    eprintln!(
        "[aetherhud] {}",
        registration_failure_message(
            &status.accelerator,
            status.error.as_deref().unwrap_or("unknown error")
        )
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_platform_default_is_registrable() {
        let accelerator = default_accelerator();
        assert!(
            canonical_accelerator(accelerator).is_ok(),
            "{accelerator} must parse as a global shortcut"
        );

        #[cfg(target_os = "macos")]
        assert_eq!(accelerator, "Option+Space");
        #[cfg(not(target_os = "macos"))]
        assert_eq!(accelerator, "Alt+`");
    }

    #[test]
    fn canonical_accelerators_normalize_modifier_and_key_names() {
        assert_eq!(canonical_accelerator("Alt+`").unwrap(), "alt+Backquote");
        assert_eq!(canonical_accelerator("option+space").unwrap(), "alt+Space");
        assert_eq!(
            canonical_accelerator("Ctrl+Shift+KeyK").unwrap(),
            "shift+control+KeyK"
        );
    }

    #[test]
    fn an_unknown_key_is_rejected_instead_of_panicking() {
        assert!(canonical_accelerator("Alt+NoSuchKey").is_err());
    }

    #[test]
    fn displays_use_the_words_style_on_windows() {
        assert_eq!(display_with("Alt+`", ModifierStyle::Words), "Alt + `");
        assert_eq!(
            display_with("Ctrl+Shift+Space", ModifierStyle::Words),
            "Ctrl + Shift + Space"
        );
        assert_eq!(
            display_with("Option+Backquote", ModifierStyle::Words),
            "Alt + `"
        );
    }

    #[test]
    fn displays_use_macos_symbols_when_asked() {
        assert_eq!(
            display_with("Option+Space", ModifierStyle::Symbols),
            "⌥Space"
        );
        assert_eq!(
            display_with("CommandOrControl+Shift+KeyK", ModifierStyle::Symbols),
            "⌘⇧K"
        );
    }

    #[test]
    fn the_failure_message_names_the_hotkey_the_reason_and_the_tray_fallback() {
        let message = registration_failure_message("Alt+`", "already registered");
        assert!(message.contains("Alt + `"));
        assert!(message.contains("already registered"));
        assert!(message.contains("托盘"));
    }

    #[test]
    fn status_snapshots_carry_both_the_raw_and_display_form() {
        let registered = HotkeyStatus::registered("Alt+`");
        assert!(registered.registered);
        assert_eq!(registered.accelerator, "Alt+`");
        assert_eq!(registered.display, display_accelerator("Alt+`"));
        assert_eq!(registered.error, None);

        let failed = HotkeyStatus::failed("Alt+`", "conflict");
        assert!(!failed.registered);
        assert_eq!(failed.error.as_deref(), Some("conflict"));
    }
}
