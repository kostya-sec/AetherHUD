//! app_data_dir 解析（Tauri 语义的本地替代实现）。
//!
//! 正式 Tauri 应用的 app_data_dir = 平台数据目录 / identifier（#10 §2）；
//! 仓库尚无 src-tauri，本工具先按平台数据目录 / AetherHUD 落盘，可用
//! --data-dir 或 AETHER_DATA_DIR 覆盖。标识符落地后应改为读取 tauri 路径。

use std::path::{Path, PathBuf};

pub const DEFAULT_APP_DIR_NAME: &str = "AetherHUD";

pub fn resolve_data_dir(explicit: Option<&Path>) -> PathBuf {
    if let Some(path) = explicit {
        return path.to_path_buf();
    }
    if let Some(env) = std::env::var_os("AETHER_DATA_DIR") {
        let path = PathBuf::from(env);
        if !path.as_os_str().is_empty() {
            return path;
        }
    }
    default_data_dir()
}

pub fn default_data_dir() -> PathBuf {
    platform_data_base().join(DEFAULT_APP_DIR_NAME)
}

#[cfg(target_os = "windows")]
fn platform_data_base() -> PathBuf {
    if let Some(appdata) = std::env::var_os("APPDATA") {
        return PathBuf::from(appdata);
    }
    home_dir()
}

#[cfg(target_os = "macos")]
fn platform_data_base() -> PathBuf {
    home_dir().join("Library/Application Support")
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn platform_data_base() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg);
    }
    home_dir().join(".local/share")
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

/// 解析评测集文件：data_dir/eval/sets/&lt;version&gt;.jsonl，
/// 边界集为 &lt;version&gt;.boundary.jsonl；版本可显式指定，否则自动发现唯一的集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetKind {
    Visible,
    Blind,
    Boundary,
}

impl SetKind {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "visible" => Ok(Self::Visible),
            "blind" => Ok(Self::Blind),
            "boundary" => Ok(Self::Boundary),
            other => Err(format!(
                "--set 只能是 visible / blind / boundary（收到 {other:?}）"
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Visible => "visible",
            Self::Blind => "blind",
            Self::Boundary => "boundary",
        }
    }

    pub fn is_boundary(&self) -> bool {
        matches!(self, Self::Boundary)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedEvalSet {
    pub path: PathBuf,
    pub eval_set_version: String,
}

pub fn resolve_eval_set_path(
    data_dir: &Path,
    kind: SetKind,
    eval_set_version: Option<&str>,
    set_file: Option<&Path>,
) -> Result<ResolvedEvalSet, String> {
    if let Some(file) = set_file {
        let version = match eval_set_version {
            Some(v) => v.to_string(),
            None => infer_version_from_path(file),
        };
        return Ok(ResolvedEvalSet {
            path: file.to_path_buf(),
            eval_set_version: version,
        });
    }
    let dir = data_dir.join("eval").join("sets");
    let suffix = if kind.is_boundary() {
        ".boundary.jsonl"
    } else {
        ".jsonl"
    };
    let version = match eval_set_version {
        Some(v) => v.to_string(),
        None => discover_single_version(&dir, kind)?,
    };
    let path = dir.join(format!("{version}{suffix}"));
    if !path.is_file() {
        return Err(format!(
            "找不到评测集文件：{}（用 --set-file 指定，或检查 --eval-set-version）",
            path.display()
        ));
    }
    Ok(ResolvedEvalSet {
        path,
        eval_set_version: version,
    })
}

fn infer_version_from_path(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    for suffix in [".boundary.jsonl", ".jsonl"] {
        if let Some(stem) = name.strip_suffix(suffix) {
            return stem.to_string();
        }
    }
    name
}

fn discover_single_version(dir: &Path, kind: SetKind) -> Result<String, String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("读取评测集目录失败（{}）：{e}", dir.display()))?;
    let mut versions: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if kind.is_boundary() {
            if let Some(stem) = name.strip_suffix(".boundary.jsonl") {
                versions.push(stem.to_string());
            }
        } else if name.ends_with(".jsonl") && !name.ends_with(".boundary.jsonl") {
            versions.push(name.trim_end_matches(".jsonl").to_string());
        }
    }
    versions.sort();
    versions.dedup();
    match versions.len() {
        0 => Err(format!(
            "{} 下没有找到评测集（用 --set-file 指定）",
            dir.display()
        )),
        1 => Ok(versions.remove(0)),
        _ => Err(format!(
            "{} 下有多个评测集版本：{}；请用 --eval-set-version 指定",
            dir.display(),
            versions.join(", ")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_kind_parses() {
        assert_eq!(SetKind::parse("visible").unwrap(), SetKind::Visible);
        assert!(SetKind::parse("all").is_err());
    }

    #[test]
    fn infers_version_from_filename() {
        assert_eq!(
            infer_version_from_path(Path::new("x/2026-10-v1.jsonl")),
            "2026-10-v1"
        );
        assert_eq!(
            infer_version_from_path(Path::new("x/2026-10-v1.boundary.jsonl")),
            "2026-10-v1"
        );
    }

    #[test]
    fn discovers_single_version() {
        let dir = std::env::temp_dir().join(format!(
            "aether-appdata-test-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        let sets = dir.join("eval").join("sets");
        std::fs::create_dir_all(&sets).unwrap();
        std::fs::write(sets.join("2026-10-v1.jsonl"), "").unwrap();
        std::fs::write(sets.join("2026-10-v1.boundary.jsonl"), "").unwrap();
        let resolved = resolve_eval_set_path(&dir, SetKind::Visible, None, None).unwrap();
        assert_eq!(resolved.eval_set_version, "2026-10-v1");
        assert!(resolved.path.ends_with("2026-10-v1.jsonl"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
