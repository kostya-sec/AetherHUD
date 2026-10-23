//! SQLite 存储边界（#27 只落 meta；#28 在同一 v1 迁移里补齐 #10 §3 全表）。
//!
//! PRAGMA 与迁移框架按 docs/spec/sqlite-schema.md：WAL、synchronous=NORMAL、
//! foreign_keys=ON、busy_timeout=5000，迁移用 PRAGMA user_version（append-only）。

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};

pub const DB_FILE: &str = "aether.db";

/// 生效分类树版本（#18 §3.4 / #10 §10）。
pub const TAXONOMY_VERSION_META_KEY: &str = "taxonomy_version";

/// 迁移 v1：T03（#27）只需要 meta 表；#28 在首次发布前把 #10 §3 的条目/标签/FTS 表
/// 并入本迁移（尚未发布的迁移历史允许改，发布后只追加）。
///
/// 迁移前 VACUUM INTO 备份（#10 §11 / AGENTS §4.3）随真实业务表一起接入：当前 v1
/// 只有 meta，没有用户数据可丢；备份策略属于 #28/#43 的存储票。
const MIGRATION_V1: &str = r#"
CREATE TABLE meta (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;
"#;

const MIGRATION_SET: &[M<'static>] = &[M::up(MIGRATION_V1)];

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("创建数据目录失败：{0}")]
    CreateDir(#[source] std::io::Error),
    #[error("打开数据库失败：{0}")]
    Open(#[source] rusqlite::Error),
    #[error("配置数据库失败：{0}")]
    Configure(#[source] rusqlite::Error),
    #[error("迁移数据库失败：{0}")]
    Migrate(#[source] rusqlite_migration::Error),
    #[error("读取 meta 失败：{0}")]
    MetaRead(#[source] rusqlite::Error),
    #[error("写入 meta 失败：{0}")]
    MetaWrite(#[source] rusqlite::Error),
    #[error("数据库连接锁已中毒")]
    Poisoned,
}

pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent).map_err(StorageError::CreateDir)?;
        }
        let mut connection = Connection::open(path).map_err(StorageError::Open)?;
        configure(&connection)?;
        migrations()
            .to_latest(&mut connection)
            .map_err(StorageError::Migrate)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    /// 测试用内存库（同一套 PRAGMA 与迁移，database 文件语义除外）。
    pub fn open_in_memory() -> Result<Self, StorageError> {
        let mut connection = Connection::open_in_memory().map_err(StorageError::Open)?;
        configure(&connection)?;
        migrations()
            .to_latest(&mut connection)
            .map_err(StorageError::Migrate)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn meta_get(&self, key: &str) -> Result<Option<String>, StorageError> {
        let connection = self.connection.lock().map_err(|_| StorageError::Poisoned)?;
        connection
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(StorageError::MetaRead)
    }

    pub fn meta_set(&self, key: &str, value: &str, now_ms: i64) -> Result<(), StorageError> {
        let connection = self.connection.lock().map_err(|_| StorageError::Poisoned)?;
        connection
            .execute(
                "INSERT INTO meta (key, value, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                rusqlite::params![key, value, now_ms],
            )
            .map_err(StorageError::MetaWrite)?;
        Ok(())
    }

    pub fn taxonomy_version(&self) -> Result<Option<String>, StorageError> {
        self.meta_get(TAXONOMY_VERSION_META_KEY)
    }

    /// 写入生效分类树版本；返回是否发生变更（version 未变时不改 updated_at）。
    pub fn set_taxonomy_version(&self, version: &str, now_ms: i64) -> Result<bool, StorageError> {
        if self.taxonomy_version()?.as_deref() == Some(version) {
            return Ok(false);
        }
        self.meta_set(TAXONOMY_VERSION_META_KEY, version, now_ms)?;
        Ok(true)
    }
}

fn configure(connection: &Connection) -> Result<(), StorageError> {
    connection
        .execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(StorageError::Configure)
}

fn migrations() -> Migrations<'static> {
    Migrations::from_slice(MIGRATION_SET)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_v1_creates_meta_and_sets_user_version() {
        let db = Database::open_in_memory().unwrap();
        let connection = db.connection.lock().unwrap();
        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(user_version, 1);
        let journal: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        // 内存库允许返回 memory；文件库必须是 wal。
        assert!(
            ["wal", "memory"].contains(&journal.as_str()),
            "journal={journal}"
        );
        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);
    }

    #[test]
    fn taxonomy_version_round_trips_and_reports_change() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(db.taxonomy_version().unwrap(), None);
        assert!(db.set_taxonomy_version("1.1.0+u0", 1000).unwrap());
        assert_eq!(db.taxonomy_version().unwrap().as_deref(), Some("1.1.0+u0"));
        assert!(!db.set_taxonomy_version("1.1.0+u0", 2000).unwrap());
        assert!(db.set_taxonomy_version("1.1.0+u1", 3000).unwrap());
        assert_eq!(db.taxonomy_version().unwrap().as_deref(), Some("1.1.0+u1"));
    }

    #[test]
    fn migrations_are_valid_for_a_fresh_database() {
        migrations().validate().unwrap();
    }
}
