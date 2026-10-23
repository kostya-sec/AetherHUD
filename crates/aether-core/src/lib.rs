//! AetherHUD 应用核心库：领域行为的唯一边界（#24 §1 / #27）。
//!
//! 负责：存储与迁移、分类树合并与生效树导出、HUD 导航数据、可注入适配器。
//! 不依赖 Tauri、不直接摸系统 API：壳把 app_data_dir、底表路径与适配器注入进来。

pub mod adapters;
pub mod navigation;
pub mod notice;
pub mod paths;
pub mod storage;
pub mod taxonomy;

use std::path::PathBuf;
use std::sync::Mutex;

use crate::adapters::{Clock, FileSystem};
use crate::navigation::HudNavigation;
use crate::notice::HudNotice;
use crate::storage::Database;
use crate::taxonomy::{TaxonomyActivation, TaxonomyPaths};

/// 启动配置：数据目录、只读底表路径、可选的数据库路径（测试用内存库/临时路径）。
#[derive(Debug, Clone)]
pub struct CoreConfig {
    pub data_dir: PathBuf,
    pub base_taxonomy: PathBuf,
    pub db_path: Option<PathBuf>,
}

impl CoreConfig {
    pub fn new(data_dir: impl Into<PathBuf>, base_taxonomy: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            base_taxonomy: base_taxonomy.into(),
            db_path: None,
        }
    }

    pub fn with_db_path(mut self, db_path: impl Into<PathBuf>) -> Self {
        self.db_path = Some(db_path.into());
        self
    }
}

/// 启动时注入的适配器；生产用真实实现，测试用记忆实现。
#[derive(Clone, Copy)]
pub struct CoreDeps<'a> {
    pub fs: &'a dyn FileSystem,
    pub clock: &'a dyn Clock,
}

#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error(transparent)]
    Taxonomy(#[from] taxonomy::TaxonomyStartupError),
    #[error(transparent)]
    Storage(#[from] storage::StorageError),
}

pub struct Core {
    activation: TaxonomyActivation,
    navigation: HudNavigation,
    database: Database,
    pending_notices: Mutex<Vec<HudNotice>>,
}

impl Core {
    /// 启动核心：分类树合并与导出、迁移数据库、写 meta.taxonomy_version。
    pub fn bootstrap(config: CoreConfig, deps: CoreDeps<'_>) -> Result<Self, CoreError> {
        let taxonomy_paths = TaxonomyPaths::new(&config.data_dir, &config.base_taxonomy);
        let activation = taxonomy::activate(&taxonomy_paths, deps.fs, deps.clock)?;
        let db_path = config
            .db_path
            .clone()
            .unwrap_or_else(|| config.data_dir.join(storage::DB_FILE));
        let database = Database::open(&db_path)?;
        database.set_taxonomy_version(activation.effective_version(), deps.clock.now_ms())?;
        let navigation = navigation::build(&activation.view);
        let pending_notices = Mutex::new(activation.notices.clone());
        Ok(Self {
            activation,
            navigation,
            database,
            pending_notices,
        })
    }

    pub fn taxonomy(&self) -> &TaxonomyActivation {
        &self.activation
    }

    /// HUD 球列/L2 面板的数据源（5 个 L1 + 形态标签，顺序即生效树顺序）。
    pub fn navigation(&self) -> &HudNavigation {
        &self.navigation
    }

    pub fn database(&self) -> &Database {
        &self.database
    }

    /// 取走启动期提示（覆盖层损坏等）；同一事件只提示一次。
    pub fn take_notices(&self) -> Vec<HudNotice> {
        match self.pending_notices.lock() {
            Ok(mut notices) => std::mem::take(&mut *notices),
            Err(_) => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::testing::{FakeClock, MemoryFileSystem};

    const BASE: &str = r#"{
      "version": "1.1.0",
      "global_l3_tags": {"format_tags": [
        {"id":"tag_tool","name":"工具"},{"id":"tag_tutorial","name":"教程"},
        {"id":"tag_prompt","name":"提示词"},{"id":"tag_workflow","name":"工作流"},
        {"id":"tag_asset","name":"素材/资源"}
      ]},
      "taxonomy": [
        {"id":"l1_inbox","name":"00 未分类","children":[{"id":"l2_manual_review","name":"待人工归档"}]},
        {"id":"l1_content","name":"01 内容与表达","children":[{"id":"l2_writing","name":"文本与写作"}]},
        {"id":"l1_productivity","name":"02 效率与工作流","children":[{"id":"l2_km","name":"信息与知识管理"}]},
        {"id":"l1_tech","name":"03 技术与工程","children":[{"id":"l2_coding","name":"编程与低代码"}]},
        {"id":"l1_biz","name":"04 商业与认知","children":[{"id":"l2_insight","name":"行业观察与研报"}]}
      ]
    }"#;

    fn temp_db_path() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "aether-core-test-{}-{}",
                std::process::id(),
                fastrand::u64(..)
            ))
            .join("aether.db")
    }

    fn config(db_path: PathBuf) -> CoreConfig {
        CoreConfig::new("/data/aether", "/install/taxonomy.json").with_db_path(db_path)
    }

    #[test]
    fn bootstrap_writes_effective_tree_meta_version_and_navigation() {
        let fs = MemoryFileSystem::new().with_file("/install/taxonomy.json", BASE);
        let clock = FakeClock::new(1_761_000_000_000);
        let deps = CoreDeps {
            fs: &fs,
            clock: &clock,
        };
        let db_path = temp_db_path();

        let core = Core::bootstrap(config(db_path.clone()), deps).unwrap();

        assert_eq!(core.navigation().l1s.len(), 5);
        assert_eq!(core.navigation().l1s[3].name, "03 技术与工程");
        assert_eq!(core.navigation().effective_version, "1.1.0+u0");
        assert_eq!(
            core.database().taxonomy_version().unwrap().as_deref(),
            Some("1.1.0+u0")
        );
        let effective = fs
            .read(&PathBuf::from("/data/aether").join(taxonomy::EFFECTIVE_FILE))
            .unwrap();
        assert!(effective.contains(r#""version": "1.1.0+u0""#));
        let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn corrupt_overlay_notice_is_delivered_once() {
        let fs = MemoryFileSystem::new()
            .with_file("/install/taxonomy.json", BASE)
            .with_file("/data/aether/taxonomy.overlay.json", "not json");
        let clock = FakeClock::new(1_761_000_000_000);
        let deps = CoreDeps {
            fs: &fs,
            clock: &clock,
        };
        let db_path = temp_db_path();

        let core = Core::bootstrap(config(db_path.clone()), deps).unwrap();
        let notices = core.take_notices();
        assert_eq!(notices.len(), 1);
        assert!(core.take_notices().is_empty());
        let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn bootstrapping_twice_is_idempotent_for_versions() {
        let fs = MemoryFileSystem::new().with_file("/install/taxonomy.json", BASE);
        let clock = FakeClock::new(1_761_000_000_000);
        let deps = CoreDeps {
            fs: &fs,
            clock: &clock,
        };
        let db_path = temp_db_path();

        let first = Core::bootstrap(config(db_path.clone()), deps).unwrap();
        let first_version = first.navigation().effective_version.clone();
        drop(first);

        let second = Core::bootstrap(config(db_path.clone()), deps).unwrap();
        assert_eq!(second.navigation().effective_version, first_version);
        assert_eq!(
            second.database().taxonomy_version().unwrap().as_deref(),
            Some(first_version.as_str())
        );
        let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
    }
}
