//! 应用数据目录解析（Tauri app_data_dir 语义的公共实现）。
//!
//! 正式应用里壳把 Tauri 的 app_data_dir 传进来；评测 CLI 与脚本用同一份解析，
//! 保证 taxonomy.overlay.json / taxonomy.effective.json / aether.db 落在同一目录。

use std::path::{Path, PathBuf};

/// 发布版标识符（tauri.conf.json）；dev 壳用 .dev 后缀的 identifier。
pub const APP_IDENTIFIER: &str = "io.github.kostya-sec.aetherhud";

/// 覆盖数据目录的环境变量（评测 CLI / 本机调试）。
pub const DATA_DIR_ENV: &str = "AETHER_DATA_DIR";

/// 数据目录：显式参数 > AETHER_DATA_DIR > 平台数据目录 / identifier。
pub fn resolve_app_data_dir(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Some(env) = std::env::var_os(DATA_DIR_ENV) {
        let path = PathBuf::from(env);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    platform_data_dir().join(APP_IDENTIFIER)
}

pub fn platform_data_dir() -> PathBuf {
    platform_data_base()
}

#[cfg(target_os = "windows")]
fn platform_data_base() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata);
    }
    home_dir().join("AppData").join("Roaming")
}

#[cfg(target_os = "macos")]
fn platform_data_base() -> PathBuf {
    home_dir().join("Library").join("Application Support")
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_data_base() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        let path = PathBuf::from(xdg);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    home_dir().join(".local").join("share")
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
fn home_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_directory_wins() {
        let explicit = Path::new("/tmp/aether-data");
        assert_eq!(resolve_app_data_dir(Some(explicit)), explicit);
    }

    #[test]
    fn default_directory_ends_with_the_app_identifier() {
        let dir = platform_data_dir().join(APP_IDENTIFIER);
        assert!(dir.ends_with(APP_IDENTIFIER));
    }
}
