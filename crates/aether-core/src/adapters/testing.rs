//! 测试夹具：确定、离线、无平台依赖的适配器实现（#24 测试决策）。

use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Mutex;

use super::{
    Clock, FileSystem, HttpError, HttpFuture, HttpMethod, HttpRequest, HttpResponse, Keychain,
    KeychainError,
};

/// 可拨动的时钟，用于覆盖层损坏快照名、meta 时间戳等确定性断言。
#[derive(Debug, Default)]
pub struct FakeClock {
    now_ms: AtomicI64,
}

impl FakeClock {
    pub fn new(now_ms: i64) -> Self {
        Self {
            now_ms: AtomicI64::new(now_ms),
        }
    }

    pub fn set(&self, now_ms: i64) {
        self.now_ms.store(now_ms, Ordering::SeqCst);
    }

    pub fn advance(&self, delta_ms: i64) {
        self.now_ms.fetch_add(delta_ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> i64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

/// 内存文件系统：测试里断言写入的 JSON、备份存在、临时文件不留痕。
#[derive(Debug, Default)]
pub struct MemoryFileSystem {
    files: Mutex<BTreeMap<PathBuf, String>>,
}

impl MemoryFileSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_file(self, path: impl Into<PathBuf>, contents: impl Into<String>) -> Self {
        self.files
            .lock()
            .unwrap()
            .insert(path.into(), contents.into());
        self
    }

    pub fn read(&self, path: &Path) -> Option<String> {
        self.files.lock().unwrap().get(path).cloned()
    }

    pub fn contains(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.files.lock().unwrap().keys().cloned().collect()
    }
}

impl FileSystem for MemoryFileSystem {
    fn exists(&self, path: &Path) -> bool {
        self.files.lock().unwrap().contains_key(path)
    }

    fn read_to_string(&self, path: &Path) -> io::Result<String> {
        self.files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{}", path.display())))
    }

    fn write_atomic(&self, path: &Path, contents: &str) -> io::Result<()> {
        self.files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), contents.to_string());
        Ok(())
    }

    fn create_dir_all(&self, _path: &Path) -> io::Result<()> {
        Ok(())
    }

    fn copy(&self, from: &Path, to: &Path) -> io::Result<u64> {
        let contents = self.read_to_string(from)?;
        let len = contents.len() as u64;
        self.files
            .lock()
            .unwrap()
            .insert(to.to_path_buf(), contents);
        Ok(len)
    }
}

/// 假 HTTP：按 (方法, URL) 注册顺序返回响应，并记录请求；没有注册响应即记为传输错误。
///
/// 抓取链里 GET 是常态；凭证验证里飞书 token 是 v1 唯一的 POST，所以按方法分队列。
/// (is_post, url) → 依次返回的响应/传输错误。
type FakeResponses = BTreeMap<(bool, String), VecDeque<Result<HttpResponse, String>>>;

#[derive(Debug, Default)]
pub struct FakeHttpTransport {
    responses: Mutex<FakeResponses>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl FakeHttpTransport {
    pub fn new() -> Self {
        Self::default()
    }

    fn enqueue_result(
        &self,
        method: HttpMethod,
        url: impl Into<String>,
        response: Result<HttpResponse, String>,
    ) {
        let is_post = method == HttpMethod::Post;
        self.responses
            .lock()
            .unwrap()
            .entry((is_post, url.into()))
            .or_default()
            .push_back(response);
    }

    /// 注册一次 GET 响应。
    pub fn enqueue(&self, url: impl Into<String>, response: HttpResponse) {
        self.enqueue_result(HttpMethod::Get, url, Ok(response));
    }

    /// 注册一次 POST 响应（凭证验证的飞书 token）。
    pub fn enqueue_post(&self, url: impl Into<String>, response: HttpResponse) {
        self.enqueue_result(HttpMethod::Post, url, Ok(response));
    }

    /// 注册一次 GET 传输错误。
    pub fn enqueue_error(&self, url: impl Into<String>, message: impl Into<String>) {
        self.enqueue_result(HttpMethod::Get, url, Err(message.into()));
    }

    /// 注册一次 POST 传输错误。
    pub fn enqueue_post_error(&self, url: impl Into<String>, message: impl Into<String>) {
        self.enqueue_result(HttpMethod::Post, url, Err(message.into()));
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }
}

impl super::HttpTransport for FakeHttpTransport {
    fn request<'a>(&'a self, request: HttpRequest) -> HttpFuture<'a> {
        self.requests.lock().unwrap().push(request.clone());
        let key = (request.method == HttpMethod::Post, request.url.clone());
        let response = self
            .responses
            .lock()
            .unwrap()
            .get_mut(&key)
            .and_then(VecDeque::pop_front);
        Box::pin(async move {
            match response {
                Some(Ok(response)) => Ok(response),
                Some(Err(message)) => Err(HttpError::Transport(message)),
                None => Err(HttpError::Transport(format!(
                    "no fake response for {} {}",
                    if request.method == HttpMethod::Post {
                        "POST"
                    } else {
                        "GET"
                    },
                    request.url
                ))),
            }
        })
    }
}

/// 内存钥匙串：只保存引用/值对，测试断言密钥不会落到别处。
#[derive(Debug, Default)]
pub struct MemoryKeychain {
    secrets: Mutex<BTreeMap<String, String>>,
}

impl MemoryKeychain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn contains(&self, account: &str) -> bool {
        self.secrets.lock().unwrap().contains_key(account)
    }
}

/// 钥匙串完全不可用（Linux 或无权限）：读/写/删都报错，供降级路径测试。
#[derive(Debug, Default, Clone, Copy)]
pub struct UnavailableKeychain;

impl Keychain for UnavailableKeychain {
    fn get_secret(&self, _account: &str) -> Result<Option<String>, KeychainError> {
        Err(KeychainError::Unavailable(
            "测试注入：钥匙串不可用".to_string(),
        ))
    }

    fn set_secret(&self, _account: &str, _secret: &str) -> Result<(), KeychainError> {
        Err(KeychainError::Unavailable(
            "测试注入：钥匙串不可用".to_string(),
        ))
    }

    fn delete_secret(&self, _account: &str) -> Result<(), KeychainError> {
        Err(KeychainError::Unavailable(
            "测试注入：钥匙串不可用".to_string(),
        ))
    }
}

impl Keychain for MemoryKeychain {
    fn get_secret(&self, account: &str) -> Result<Option<String>, KeychainError> {
        Ok(self.secrets.lock().unwrap().get(account).cloned())
    }

    fn set_secret(&self, account: &str, secret: &str) -> Result<(), KeychainError> {
        self.secrets
            .lock()
            .unwrap()
            .insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete_secret(&self, account: &str) -> Result<(), KeychainError> {
        self.secrets.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_fs_persists_and_copies() {
        let fs = MemoryFileSystem::new();
        fs.write_atomic(Path::new("/data/overlay.json"), "x")
            .unwrap();
        assert_eq!(
            fs.read(Path::new("/data/overlay.json")).as_deref(),
            Some("x")
        );
        fs.copy(
            Path::new("/data/overlay.json"),
            Path::new("/backup/overlay.json"),
        )
        .unwrap();
        assert!(fs.contains(Path::new("/backup/overlay.json")));
        assert_eq!(
            fs.read_to_string(Path::new("/missing.json"))
                .unwrap_err()
                .kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn fake_clock_is_controllable() {
        let clock = FakeClock::new(1000);
        assert_eq!(clock.now_ms(), 1000);
        clock.advance(250);
        assert_eq!(clock.now_ms(), 1250);
        clock.set(42);
        assert_eq!(clock.now_ms(), 42);
    }

    #[test]
    fn memory_keychain_round_trips_and_deletes() {
        let keychain = MemoryKeychain::new();
        keychain.set_secret("jina.api_key", "secret").unwrap();
        assert_eq!(
            keychain.get_secret("jina.api_key").unwrap().as_deref(),
            Some("secret")
        );
        keychain.delete_secret("jina.api_key").unwrap();
        assert_eq!(keychain.get_secret("jina.api_key").unwrap(), None);
    }
}
