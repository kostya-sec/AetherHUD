//! 壳侧核心库装配：数据目录与底表路径解析、`aether-core` 启动、IPC 数据出口。
//!
//! 这里是 Tauri 与领域逻辑之间唯一的装配层；命令只转发，不含业务规则（#24 §1）。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Manager};

use aether_core::adapters::{OsKeychain, ReqwestTransport, StdFileSystem, SystemClock};
use aether_core::credentials::{
    CredentialKind, CredentialService, CredentialStatus, ValidationOutcome, ValidationTarget,
};
use aether_core::inbox::{InboxCursor, InboxPage};
use aether_core::ingest::IngestOutcome;
use aether_core::navigation::HudNavigation;
use aether_core::notice::HudNotice;
use aether_core::{Core, CoreConfig, CoreDeps};

/// 本机调试用的底表覆盖路径（发布版不需要设置）。
pub const BASE_TAXONOMY_ENV: &str = "AETHERHUD_TAXONOMY";

/// 分类树启动结果；失败时保留原因，HUD 仍可运行（球列退回占位）。
pub struct CoreState {
    core: Option<Core>,
    error: Option<String>,
    /// 启动失败提示只给一次（每次唤出都刷同一条会烦人）。
    error_notice_taken: AtomicBool,
    /// 系统钥匙串适配器（#15 / #29）：领域层只看到 trait，壳在这里注入。
    keychain: OsKeychain,
    /// 时钟适配器（凭证引用写入时间；测试注入记忆时钟）。
    clock: SystemClock,
}

impl CoreState {
    pub fn bootstrap(app: &AppHandle) -> Self {
        let base_taxonomy = match resolve_base_taxonomy(app) {
            Ok(path) => path,
            Err(error) => {
                eprintln!("[aetherhud] {error}");
                return Self {
                    core: None,
                    error: Some(error),
                    error_notice_taken: AtomicBool::new(false),
                    keychain: OsKeychain,
                    clock: SystemClock,
                };
            }
        };
        let data_dir = match std::env::var_os(aether_core::paths::DATA_DIR_ENV) {
            // 开发/冒烟测试覆盖：与 aether-eval 的 AETHER_DATA_DIR 指向同一目录。
            Some(value) if !value.is_empty() => PathBuf::from(value),
            _ => match app.path().app_data_dir() {
                Ok(path) => path,
                Err(error) => {
                    let message = format!("无法解析 app_data_dir：{error}");
                    eprintln!("[aetherhud] {message}");
                    return Self {
                        core: None,
                        error: Some(message),
                        error_notice_taken: AtomicBool::new(false),
                        keychain: OsKeychain,
                        clock: SystemClock,
                    };
                }
            },
        };
        let config = CoreConfig::new(&data_dir, &base_taxonomy);
        let fs = StdFileSystem;
        let clock = SystemClock;
        let deps = CoreDeps {
            fs: &fs,
            clock: &clock,
        };
        match Core::bootstrap(config, deps) {
            Ok(core) => {
                eprintln!(
                    "[aetherhud] 分类树就绪：{}（底表 {}，数据目录 {}）",
                    core.navigation().effective_version,
                    core.navigation().base_version,
                    data_dir.display()
                );
                Self {
                    core: Some(core),
                    error: None,
                    error_notice_taken: AtomicBool::new(false),
                    keychain: OsKeychain,
                    clock: SystemClock,
                }
            }
            Err(error) => {
                let message = format!("核心库启动失败：{error}");
                eprintln!("[aetherhud] {message}");
                Self {
                    core: None,
                    error: Some(message),
                    error_notice_taken: AtomicBool::new(false),
                    keychain: OsKeychain,
                    clock: SystemClock,
                }
            }
        }
    }

    pub fn navigation(&self) -> Result<HudNavigation, String> {
        Ok(self.core()?.navigation().clone())
    }

    /// 本地录入（#28）：入库成功即代表 entries + entry_sources + pipeline_jobs 已同事务写入。
    pub fn ingest_text(&self, text: &str, now_ms: i64) -> Result<IngestOutcome, String> {
        self.core()?
            .ingest_text(text, now_ms)
            .map_err(|error| error.to_string())
    }

    /// 收件箱一页（50 条/页 keyset 分页）。
    pub fn inbox_page(&self, cursor: Option<InboxCursor>) -> Result<InboxPage, String> {
        self.core()?
            .inbox_page(cursor)
            .map_err(|error| error.to_string())
    }

    /// 凭证区四行状态：已配置 / 未配置 / 钥匙串不可用（#29）。
    pub fn credential_statuses(&self) -> Result<Vec<CredentialStatus>, String> {
        Ok(self.credential_service()?.statuses())
    }

    /// 保存一项凭证：密钥进系统钥匙串，设置表只落 `keychain://` 引用。
    pub fn save_credential(
        &self,
        kind: CredentialKind,
        secret: &str,
    ) -> Result<CredentialStatus, String> {
        self.credential_service()?
            .save(kind, secret)
            .map_err(|error| error.to_string())
    }

    /// 清除一项凭证（幂等）；只删密钥与引用，不碰条目数据。
    pub fn clear_credential(&self, kind: CredentialKind) -> Result<CredentialStatus, String> {
        self.credential_service()?
            .clear(kind)
            .map_err(|error| error.to_string())
    }

    /// 验证凭证连通性（百炼 / Jina / 飞书）；失败返回可读原因，不返回密钥。
    pub async fn validate_credential(
        &self,
        target: ValidationTarget,
    ) -> Result<ValidationOutcome, String> {
        let transport = ReqwestTransport::new();
        Ok(self
            .credential_service()?
            .validate(target, &transport)
            .await)
    }

    /// Keychain + settings + 时钟的组合边界；壳只注入适配器，业务规则全在 core。
    fn credential_service(&self) -> Result<CredentialService<'_>, String> {
        let core = self.core()?;
        Ok(CredentialService::new(
            &self.keychain,
            core.database(),
            &self.clock,
        ))
    }

    fn core(&self) -> Result<&Core, String> {
        self.core.as_ref().ok_or_else(|| {
            self.error
                .clone()
                .unwrap_or_else(|| "核心库尚未初始化".to_string())
        })
    }

    /// 启动期提示（覆盖层损坏等）；前端在每次唤出后取走并显示 5 秒提示条。
    ///
    /// 核心库启动失败时同样产出一条提示（只一次），否则用户只看到占位球、无从诊断。
    pub fn take_notices(&self) -> Vec<HudNotice> {
        if let Some(core) = &self.core {
            return core.take_notices();
        }
        if self.error_notice_taken.swap(true, Ordering::SeqCst) {
            return Vec::new();
        }
        let reason = self.error.as_deref().unwrap_or("核心库尚未初始化");
        vec![HudNotice::core_unavailable(reason)]
    }
}

/// 底表解析顺序：环境变量 → 安装包资源目录 → 可执行文件旁 →（仅 debug）仓库根/当前目录。
///
/// 发布版不读当前目录：底表是安装包资源（#18 §3.2），避免从任意工作目录加载伪造分类树；
/// 开发期 cargo build 的 debug 载体没有资源副本，才用编译期仓库路径兜底。
pub fn resolve_base_taxonomy(app: &AppHandle) -> Result<PathBuf, String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(explicit) = std::env::var_os(BASE_TAXONOMY_ENV) {
        let path = PathBuf::from(explicit);
        if !path.as_os_str().is_empty() {
            candidates.push(path);
        }
    }
    if let Ok(resource_dir) = app.path().resource_dir() {
        candidates.push(resource_dir.join("taxonomy.json"));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            candidates.push(parent.join("taxonomy.json"));
        }
    }
    #[cfg(debug_assertions)]
    {
        if let Some(manifest_dir) = option_env!("CARGO_MANIFEST_DIR") {
            if let Some(repo_root) = Path::new(manifest_dir).parent() {
                candidates.push(repo_root.join("taxonomy.json"));
            }
        }
        if let Ok(current_dir) = std::env::current_dir() {
            candidates.push(current_dir.join("taxonomy.json"));
        }
    }
    pick_base_taxonomy(candidates.iter().map(PathBuf::as_path)).ok_or_else(|| {
        let tried = candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join("、");
        format!("找不到只读底表 taxonomy.json（试过：{tried}）")
    })
}

/// 取第一个存在的候选路径；抽成纯函数便于单测（不依赖 Tauri 运行时）。
pub fn pick_base_taxonomy<'a>(candidates: impl IntoIterator<Item = &'a Path>) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|path| path.is_file())
        .map(Path::to_path_buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_first_existing_candidate() {
        let dir = std::env::temp_dir().join(format!(
            "aether-base-tax-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let missing = dir.join("missing.json");
        let present = dir.join("taxonomy.json");
        std::fs::write(&present, "{}").unwrap();

        let picked = pick_base_taxonomy([missing.as_path(), present.as_path()]).unwrap();
        assert_eq!(picked, present);
        assert!(pick_base_taxonomy([missing.as_path()]).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
