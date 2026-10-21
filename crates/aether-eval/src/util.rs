//! 通用工具：时间戳、哈希、无覆盖写入、格式化。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use sha2::{Digest, Sha256};

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(sha256_hex(&bytes))
}

pub fn ensure_dir(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

/// 只创建不覆盖：报告 / 基线文件不得覆盖已有文件（#14 §4.4）。
pub fn write_new(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(content.as_bytes())
}

/// 基线索引等可重建文件：临时文件 + rename 原子替换。
pub fn write_atomic(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        ensure_dir(parent)?;
    }
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)
}

pub fn now_rfc3339() -> String {
    chrono::Local::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, false)
}

pub fn file_timestamp() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}

pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

pub fn pct(part: usize, total: usize) -> Option<f64> {
    if total == 0 {
        None
    } else {
        Some(part as f64 / total as f64)
    }
}

pub fn fmt_pct(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.1}%", v * 100.0),
        None => "—".to_string(),
    }
}

pub fn fmt_rate(value: Option<f64>) -> String {
    match value {
        Some(v) => format!("{:.3}", v),
        None => "—".to_string(),
    }
}

pub fn fmt_delta(value: Option<f64>, as_percent: bool) -> String {
    match value {
        Some(v) => {
            if as_percent {
                format!("{}{:.1}pt", if v >= 0.0 { "+" } else { "" }, v * 100.0)
            } else {
                format!("{}{:.3}", if v >= 0.0 { "+" } else { "" }, v)
            }
        }
        None => "—".to_string(),
    }
}

pub fn fmt_duration_ms(ms: u64) -> String {
    if ms < 1000 {
        format!("{ms}ms")
    } else {
        let secs = ms as f64 / 1000.0;
        if secs < 60.0 {
            format!("{secs:.1}s")
        } else {
            format!("{:.0}m{:.0}s", secs / 60.0, secs % 60.0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn write_new_refuses_overwrite() {
        let path = std::env::temp_dir().join(format!(
            "aether-util-test-{}-{}.txt",
            std::process::id(),
            fastrand::u64(..)
        ));
        write_new(&path, "a").unwrap();
        let err = write_new(&path, "b").unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn timestamp_looks_right() {
        let ts = file_timestamp();
        assert_eq!(ts.len(), 15);
        assert_eq!(&ts[8..9], "-");
    }
}
