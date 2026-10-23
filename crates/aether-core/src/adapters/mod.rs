//! 可注入适配器：领域逻辑只通过这里接触外部世界（#24 §1 / #27）。
//!
//! 真实实现是薄适配器（系统时钟、std 文件系统、reqwest、系统钥匙串）；
//! 测试用 `testing` 下的记忆实现，离线、确定、可重复。

pub mod testing;

use std::future::Future;
use std::io;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 对外部时间的唯一入口（Unix epoch 毫秒 UTC）。
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> i64;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// 文件系统边界：分类树覆盖层/生效树导出、备份与数据库目录都用它。
///
/// 数据库文件本身由 rusqlite 直接打开，不经过本 trait（SQLite 需要原生文件语义）。
pub trait FileSystem: Send + Sync {
    fn exists(&self, path: &Path) -> bool;
    fn read_to_string(&self, path: &Path) -> io::Result<String>;
    /// 临时文件 + 原子 rename（Windows 与 Unix 的 rename 都会替换目标）。
    fn write_atomic(&self, path: &Path, contents: &str) -> io::Result<()>;
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct StdFileSystem;

static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

impl FileSystem for StdFileSystem {
    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write_atomic(&self, path: &Path, contents: &str) -> io::Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)?;
        }
        let temp = temp_path(path);
        std::fs::write(&temp, contents)?;
        match std::fs::rename(&temp, path) {
            Ok(()) => Ok(()),
            Err(error) => {
                let _ = std::fs::remove_file(&temp);
                Err(error)
            }
        }
    }

    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        if let Some(parent) = to.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(from, to)
    }
}

fn temp_path(path: &Path) -> PathBuf {
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "aether".to_string());
    let temp_name = format!(".{file_name}.tmp-{}-{sequence}", std::process::id());
    match path.parent() {
        Some(parent) => parent.join(temp_name),
        None => PathBuf::from(temp_name),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub timeout_ms: u64,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            headers: Vec::new(),
            timeout_ms: 20_000,
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.push((name.into(), value.into()));
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP 请求失败：{0}")]
    Transport(String),
    #[error("HTTP 响应读取失败：{0}")]
    Body(String),
}

pub type HttpFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HttpResponse, HttpError>> + Send + 'a>>;

/// 抓取/同步等出站 HTTP 的唯一边界（#27 只建立适配器，抓取链在后续票接入）。
pub trait HttpTransport: Send + Sync {
    fn get<'a>(&'a self, request: HttpRequest) -> HttpFuture<'a>;
}

#[derive(Debug, Clone, Default)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

impl HttpTransport for ReqwestTransport {
    fn get<'a>(&'a self, request: HttpRequest) -> HttpFuture<'a> {
        Box::pin(async move {
            let mut builder = self
                .client
                .get(&request.url)
                .timeout(Duration::from_millis(request.timeout_ms.max(1)));
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            let response = builder
                .send()
                .await
                .map_err(|error| HttpError::Transport(error.to_string()))?;
            let status = response.status().as_u16();
            let headers = response
                .headers()
                .iter()
                .filter_map(|(name, value)| {
                    value
                        .to_str()
                        .ok()
                        .map(|value| (name.to_string(), value.to_string()))
                })
                .collect();
            let body = response
                .text()
                .await
                .map_err(|error| HttpError::Body(error.to_string()))?;
            Ok(HttpResponse {
                status,
                headers,
                body,
            })
        })
    }
}

/// 系统钥匙串的 service 名（#15）：DB/settings 只存 `keychain://aetherhud/<account>` 引用。
pub const KEYCHAIN_SERVICE: &str = "aetherhud";

/// 把 account 变成可入库的引用串；密钥本体永不进 DB/日志。
pub fn secret_ref(account: &str) -> String {
    format!("keychain://{KEYCHAIN_SERVICE}/{account}")
}

/// 解析引用串，返回 account；不是本应用引用时返回 None。
pub fn parse_secret_ref(reference: &str) -> Option<&str> {
    reference
        .strip_prefix("keychain://")
        .and_then(|rest| rest.strip_prefix(KEYCHAIN_SERVICE))
        .and_then(|rest| rest.strip_prefix('/'))
        .filter(|account| !account.is_empty())
}

#[derive(Debug, thiserror::Error)]
pub enum KeychainError {
    #[error("钥匙串不可用：{0}")]
    Unavailable(String),
    #[error("钥匙串操作失败：{0}")]
    Platform(String),
}

/// 密钥读写的唯一边界（#15）；实现方只暴露引用串，日志里永远没有明文。
pub trait Keychain: Send + Sync {
    fn get_secret(&self, account: &str) -> Result<Option<String>, KeychainError>;
    fn set_secret(&self, account: &str, secret: &str) -> Result<(), KeychainError>;
    fn delete_secret(&self, account: &str) -> Result<(), KeychainError>;
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
#[derive(Debug, Clone, Copy, Default)]
pub struct OsKeychain;

#[cfg(any(target_os = "windows", target_os = "macos"))]
impl Keychain for OsKeychain {
    fn get_secret(&self, account: &str) -> Result<Option<String>, KeychainError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .map_err(|error| KeychainError::Platform(error.to_string()))?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(KeychainError::Platform(error.to_string())),
        }
    }

    fn set_secret(&self, account: &str, secret: &str) -> Result<(), KeychainError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .map_err(|error| KeychainError::Platform(error.to_string()))?;
        entry
            .set_password(secret)
            .map_err(|error| KeychainError::Platform(error.to_string()))
    }

    fn delete_secret(&self, account: &str) -> Result<(), KeychainError> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .map_err(|error| KeychainError::Platform(error.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(KeychainError::Platform(error.to_string())),
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
#[derive(Debug, Clone, Copy, Default)]
pub struct OsKeychain;

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
impl Keychain for OsKeychain {
    fn get_secret(&self, _account: &str) -> Result<Option<String>, KeychainError> {
        Err(KeychainError::Unavailable(
            "当前平台未接入系统钥匙串（v1 仅 Windows/macOS）".to_string(),
        ))
    }

    fn set_secret(&self, _account: &str, _secret: &str) -> Result<(), KeychainError> {
        Err(KeychainError::Unavailable(
            "当前平台未接入系统钥匙串（v1 仅 Windows/macOS）".to_string(),
        ))
    }

    fn delete_secret(&self, _account: &str) -> Result<(), KeychainError> {
        Err(KeychainError::Unavailable(
            "当前平台未接入系统钥匙串（v1 仅 Windows/macOS）".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn std_fs_atomic_write_replaces_existing_and_leaves_no_temp() {
        let dir = std::env::temp_dir().join(format!(
            "aether-fs-test-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        fs::create_dir_all(dir.join("nested")).unwrap();
        let path = dir.join("nested").join("taxonomy.effective.json");
        let fs_port = StdFileSystem;

        fs_port.write_atomic(&path, "{\"version\":1}").unwrap();
        fs_port.write_atomic(&path, "{\"version\":2}").unwrap();

        assert_eq!(fs_port.read_to_string(&path).unwrap(), "{\"version\":2}");
        let leftovers: Vec<_> = fs::read_dir(dir.join("nested"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().to_string())
            .filter(|name| name != "taxonomy.effective.json")
            .collect();
        assert!(leftovers.is_empty(), "unexpected temp files: {leftovers:?}");
    }

    #[test]
    fn secret_refs_round_trip() {
        assert_eq!(
            secret_ref("classifier.api_key"),
            "keychain://aetherhud/classifier.api_key"
        );
        assert_eq!(
            parse_secret_ref("keychain://aetherhud/classifier.api_key"),
            Some("classifier.api_key")
        );
        assert_eq!(
            parse_secret_ref("keychain://other/classifier.api_key"),
            None
        );
        assert_eq!(parse_secret_ref("classifier.api_key"), None);
    }

    #[test]
    fn system_clock_returns_epoch_milliseconds() {
        assert!(SystemClock.now_ms() > 1_700_000_000_000);
    }
}
