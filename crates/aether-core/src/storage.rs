//! SQLite 存储边界（#10 schema / #28 v1 全表落地）。
//!
//! PRAGMA 与迁移框架按 docs/spec/sqlite-schema.md：WAL、synchronous=NORMAL、
//! foreign_keys=ON、busy_timeout=5000，迁移用 PRAGMA user_version（append-only）。
//! 条目写入、去重双轨与收件箱分页都在本模块；业务语义（URL 归一化、录入计划）
//! 在 crate::url / crate::ingest，壳只做 IPC 转发。

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{Migrations, M};

use crate::inbox::{InboxCursor, InboxEntry, InboxPage};
use crate::ingest::{IngestOutcome, IngestPlan};

pub const DB_FILE: &str = "aether.db";

/// 生效分类树版本（#18 §3.4 / #10 §10）。
pub const TAXONOMY_VERSION_META_KEY: &str = "taxonomy_version";

/// 迁移 v1：docs/spec/sqlite-schema.md §3–§4 的全部 DDL（表 + 索引 + FTS 触发器）。
///
/// v1 尚未发布，条目/标签/FTS 表在 #28 并入本迁移（#27 只落过 meta 的临时 v1
/// 没有用户数据可保留）。发布后本迁移只追加、不再修改。
const MIGRATION_V1: &str = r#"
-- meta（内部元数据）
CREATE TABLE meta (
  key        TEXT PRIMARY KEY,
  value      TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

-- settings（非机密配置；密钥本体只进 OS Keychain）
CREATE TABLE settings (
  key        TEXT PRIMARY KEY,
  value_json TEXT NOT NULL,
  updated_at INTEGER NOT NULL
) WITHOUT ROWID;

-- entries（条目）
CREATE TABLE entries (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  uid             TEXT    NOT NULL UNIQUE,
  title           TEXT    NOT NULL,
  url             TEXT,
  canonical_url   TEXT,
  url_hash        BLOB,
  media_type      TEXT    NOT NULL DEFAULT 'link'
                          CHECK (media_type IN ('link','video','article')),
  author          TEXT,
  l1_id           TEXT    NOT NULL,
  l2_id           TEXT    NOT NULL,
  body_md         TEXT,
  excerpt         TEXT,
  notes           TEXT,
  pipeline_status TEXT    NOT NULL DEFAULT 'pending'
                          CHECK (pipeline_status IN ('pending','processing','classified','failed')),
  failure_reason  TEXT    CHECK (failure_reason IN ('fetch_failed','classify_failed')),
  metadata_status TEXT    NOT NULL DEFAULT 'ok'
                          CHECK (metadata_status IN ('ok','partial','failed')),
  lifecycle       TEXT    NOT NULL DEFAULT 'active'
                          CHECK (lifecycle IN ('active','archived','trashed')),
  pinned          INTEGER NOT NULL DEFAULT 0 CHECK (pinned IN (0,1)),
  done            INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0,1)),
  corrected_by    TEXT    CHECK (corrected_by IN ('ai','human')),
  classify_suggestion TEXT,
  fetch_source    TEXT    CHECK (fetch_source IN ('dom_smoothie','jina','video_api','none')),
  fetched_at      INTEGER,
  trashed_at      INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  last_seen_at    INTEGER NOT NULL
);

-- tags（L3 标签实体，全局唯一）
CREATE TABLE tags (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  kind            TEXT    NOT NULL CHECK (kind IN ('format','feature')),
  origin          TEXT    NOT NULL CHECK (origin IN ('format','preset','ai','user')),
  display_name    TEXT    NOT NULL,
  normalized_key  TEXT    NOT NULL,
  color           TEXT,
  is_resident     INTEGER NOT NULL DEFAULT 0 CHECK (is_resident IN (0,1)),
  is_hidden       INTEGER NOT NULL DEFAULT 0 CHECK (is_hidden IN (0,1)),
  deleted_at      INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_tags_normalized_key ON tags (normalized_key);

-- tag_aliases（别名表）
CREATE TABLE tag_aliases (
  alias_normalized TEXT PRIMARY KEY,
  tag_id           INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  alias_display    TEXT    NOT NULL,
  created_at       INTEGER NOT NULL
);
CREATE INDEX ix_tag_aliases_tag ON tag_aliases (tag_id);

-- entry_tags（条目-标签关联）
CREATE TABLE entry_tags (
  entry_id    INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  tag_id      INTEGER NOT NULL REFERENCES tags(id)   ON DELETE CASCADE,
  assigned_by TEXT    NOT NULL CHECK (assigned_by IN ('ai','human')),
  created_at  INTEGER NOT NULL,
  PRIMARY KEY (entry_id, tag_id)
) WITHOUT ROWID;
CREATE INDEX ix_entry_tags_tag ON entry_tags (tag_id, entry_id);

-- entry_sources（摄取来源，多来源一条目）
CREATE TABLE entry_sources (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id    INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  source_type TEXT    NOT NULL,
  source_ref  TEXT    NOT NULL,
  ingested_at INTEGER NOT NULL,
  last_seen_at INTEGER NOT NULL
);
CREATE UNIQUE INDEX ux_entry_sources_ref ON entry_sources (source_type, source_ref);
CREATE INDEX ix_entry_sources_entry ON entry_sources (entry_id);

-- corrections（纠偏记录，匿名保留）
CREATE TABLE corrections (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id         INTEGER REFERENCES entries(id) ON DELETE SET NULL,
  dimension        TEXT    NOT NULL
                           CHECK (dimension IN ('l1','l2','tag_add','tag_remove','media_type')),
  before_value     TEXT,
  after_value      TEXT,
  trigger          TEXT    NOT NULL CHECK (trigger IN ('manual_edit','reclassify')),
  taxonomy_version TEXT,
  created_at       INTEGER NOT NULL
);
CREATE INDEX ix_corrections_entry   ON corrections (entry_id, created_at DESC);
CREATE INDEX ix_corrections_created ON corrections (created_at);

-- pipeline_jobs（执行队列；策略归 #11）
CREATE TABLE pipeline_jobs (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  entry_id        INTEGER NOT NULL REFERENCES entries(id) ON DELETE CASCADE,
  stage           TEXT    NOT NULL CHECK (stage IN ('fetch','classify')),
  state           TEXT    NOT NULL DEFAULT 'queued' CHECK (state IN ('queued','running')),
  attempts        INTEGER NOT NULL DEFAULT 0,
  next_attempt_at INTEGER NOT NULL,
  lease_until     INTEGER,
  last_error      TEXT,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  UNIQUE (entry_id, stage)
);
CREATE INDEX ix_pipeline_jobs_ready ON pipeline_jobs (next_attempt_at) WHERE state = 'queued';
CREATE INDEX ix_pipeline_jobs_lease ON pipeline_jobs (lease_until)     WHERE state = 'running';

-- entries 索引（#10 §4）
CREATE UNIQUE INDEX ux_entries_url_hash ON entries (url_hash) WHERE url_hash IS NOT NULL;
CREATE INDEX ix_entries_l1_l2          ON entries (lifecycle, l1_id, l2_id, pinned DESC, created_at DESC);
CREATE INDEX ix_entries_pipeline       ON entries (pipeline_status, created_at DESC);
CREATE INDEX ix_entries_lifecycle_created ON entries (lifecycle, created_at DESC);
CREATE INDEX ix_entries_trashed        ON entries (lifecycle, trashed_at) WHERE lifecycle = 'trashed';

-- FTS5（external content，trigram；只索引 title/excerpt/notes/author）
CREATE VIRTUAL TABLE fts_entries USING fts5(
  title,
  excerpt,
  notes,
  author,
  content = 'entries',
  content_rowid = 'id',
  tokenize = 'trigram'
);

CREATE TRIGGER entries_ai AFTER INSERT ON entries BEGIN
  INSERT INTO fts_entries (rowid, title, excerpt, notes, author)
  VALUES (new.id, new.title, new.excerpt, new.notes, new.author);
END;

CREATE TRIGGER entries_ad AFTER DELETE ON entries BEGIN
  INSERT INTO fts_entries (fts_entries, rowid, title, excerpt, notes, author)
  VALUES ('delete', old.id, old.title, old.excerpt, old.notes, old.author);
END;

CREATE TRIGGER entries_au AFTER UPDATE OF title, excerpt, notes, author ON entries BEGIN
  INSERT INTO fts_entries (fts_entries, rowid, title, excerpt, notes, author)
  VALUES ('delete', old.id, old.title, old.excerpt, old.notes, old.author);
  INSERT INTO fts_entries (rowid, title, excerpt, notes, author)
  VALUES (new.id, new.title, new.excerpt, new.notes, new.author);
END;

-- 首建后 rebuild 一次（触发器只对建立后的写入生效）。
INSERT INTO fts_entries(fts_entries) VALUES('rebuild');
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
    #[error("写入条目失败：{0}")]
    Ingest(#[source] rusqlite::Error),
    #[error("读取设置失败：{0}")]
    SettingsRead(#[source] rusqlite::Error),
    #[error("写入设置失败：{0}")]
    SettingsWrite(#[source] rusqlite::Error),
    #[error("设置值不是合法 JSON：{0}")]
    SettingsJson(#[source] serde_json::Error),
    #[error("读取收件箱失败：{0}")]
    Inbox(#[source] rusqlite::Error),
    #[error("数据库连接锁已中毒")]
    Poisoned,
    #[error(
        "检测到旧的开发期 v1 数据库（缺少 entries 等表）：{path}\nv1 尚未发布，请删除该文件（连同同目录的 aether.db-wal / aether.db-shm）后重启"
    )]
    StaleDevelopmentDatabase { path: std::path::PathBuf },
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
        if !schema_is_complete(&connection).map_err(StorageError::Open)? {
            // #27 的临时 v1 只有 meta；v1 尚未发布，直接给出可操作错误而不是让
            // 后续 INSERT 报 "no such table: entries"。
            return Err(StorageError::StaleDevelopmentDatabase {
                path: path.to_path_buf(),
            });
        }
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
        let connection = self.lock()?;
        connection
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(StorageError::MetaRead)
    }

    pub fn meta_set(&self, key: &str, value: &str, now_ms: i64) -> Result<(), StorageError> {
        let connection = self.lock()?;
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

    /// 读取设置项（value_json 反序列化）。未设置返回 None；坏 JSON 给出可读错误。
    pub fn setting_get<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, StorageError> {
        let raw = {
            let connection = self.lock()?;
            connection
                .query_row(
                    "SELECT value_json FROM settings WHERE key = ?1",
                    [key],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(StorageError::SettingsRead)?
        };
        match raw {
            Some(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(StorageError::SettingsJson),
            None => Ok(None),
        }
    }

    /// 写入设置项（value_json 序列化）；key 是主键，覆盖写不产生第二行。
    pub fn setting_set<T: serde::Serialize>(
        &self,
        key: &str,
        value: &T,
        now_ms: i64,
    ) -> Result<(), StorageError> {
        let value_json = serde_json::to_string(value).map_err(StorageError::SettingsJson)?;
        let connection = self.lock()?;
        connection
            .execute(
                "INSERT INTO settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                              updated_at = excluded.updated_at",
                params![key, value_json, now_ms],
            )
            .map_err(StorageError::SettingsWrite)?;
        Ok(())
    }

    /// 删除设置项；返回是否删掉了既有行（清除未配置的项不报错）。
    pub fn setting_delete(&self, key: &str) -> Result<bool, StorageError> {
        let connection = self.lock()?;
        let changed = connection
            .execute("DELETE FROM settings WHERE key = ?1", [key])
            .map_err(StorageError::SettingsWrite)?;
        Ok(changed > 0)
    }

    /// 本地录入：同一事务内完成双轨去重 + 条目/来源/队列写入（#28）。
    ///
    /// 去重顺序按 #10 §8：先来源外部 id，再 canonical URL 哈希；命中只更新
    /// last_seen_at 与来源时间，不新建条目、不入队新 job。
    pub fn ingest(&self, plan: &IngestPlan, now_ms: i64) -> Result<IngestOutcome, StorageError> {
        let mut connection = self.lock()?;
        let transaction = connection.transaction().map_err(StorageError::Ingest)?;

        if let Some(existing) = find_by_source(&transaction, &plan.source_type, &plan.source_ref)
            .map_err(StorageError::Ingest)?
        {
            let last_seen_at =
                touch_entry_last_seen(&transaction, existing.id, existing.last_seen_at, now_ms)
                    .map_err(StorageError::Ingest)?;
            transaction
                .execute(
                    "UPDATE entry_sources SET last_seen_at = ?1
                     WHERE source_type = ?2 AND source_ref = ?3",
                    params![now_ms, plan.source_type, plan.source_ref],
                )
                .map_err(StorageError::Ingest)?;
            transaction.commit().map_err(StorageError::Ingest)?;
            return Ok(duplicate_outcome(existing, last_seen_at));
        }

        if let Some(hash) = &plan.url_hash {
            if let Some(existing) =
                find_by_url_hash(&transaction, hash).map_err(StorageError::Ingest)?
            {
                let last_seen_at =
                    touch_entry_last_seen(&transaction, existing.id, existing.last_seen_at, now_ms)
                        .map_err(StorageError::Ingest)?;
                transaction
                    .execute(
                        "INSERT INTO entry_sources
                           (entry_id, source_type, source_ref, ingested_at, last_seen_at)
                         VALUES (?1, ?2, ?3, ?4, ?4)",
                        params![existing.id, plan.source_type, plan.source_ref, now_ms],
                    )
                    .map_err(StorageError::Ingest)?;
                transaction.commit().map_err(StorageError::Ingest)?;
                return Ok(duplicate_outcome(existing, last_seen_at));
            }
        }

        transaction
            .execute(
                "INSERT INTO entries (
                   uid, title, url, canonical_url, url_hash, media_type,
                   l1_id, l2_id, body_md, excerpt,
                   pipeline_status, metadata_status,
                   created_at, updated_at, last_seen_at
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6,
                   ?7, ?8, ?9, ?10,
                   'pending', 'ok',
                   ?11, ?11, ?11
                 )",
                params![
                    plan.uid,
                    plan.title,
                    plan.url,
                    plan.canonical_url,
                    plan.url_hash.as_ref().map(|hash| hash.as_slice()),
                    plan.media_type.as_str(),
                    plan.l1_id,
                    plan.l2_id,
                    plan.body_md,
                    plan.excerpt,
                    now_ms,
                ],
            )
            .map_err(StorageError::Ingest)?;
        let entry_id = transaction.last_insert_rowid();

        transaction
            .execute(
                "INSERT INTO entry_sources
                   (entry_id, source_type, source_ref, ingested_at, last_seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                params![entry_id, plan.source_type, plan.source_ref, now_ms],
            )
            .map_err(StorageError::Ingest)?;

        transaction
            .execute(
                "INSERT INTO pipeline_jobs
                   (entry_id, stage, state, attempts, next_attempt_at, created_at, updated_at)
                 VALUES (?1, ?2, 'queued', 0, ?3, ?3, ?3)",
                params![entry_id, plan.stage.as_str(), now_ms],
            )
            .map_err(StorageError::Ingest)?;

        transaction.commit().map_err(StorageError::Ingest)?;

        Ok(IngestOutcome {
            entry_id,
            duplicate: false,
            stage: Some(plan.stage),
            pipeline_status: "pending".to_string(),
            title: plan.title.clone(),
            created_at: now_ms,
            last_seen_at: now_ms,
        })
    }

    /// 收件箱一页：pipeline_status ∈ (pending, processing, failed)，created_at 倒序。
    ///
    /// 多取一行判断是否还有下一页；游标是上一页最后一行 (created_at, id)。
    pub fn inbox_page(
        &self,
        cursor: Option<InboxCursor>,
        limit: usize,
    ) -> Result<InboxPage, StorageError> {
        let limit = limit.clamp(1, 200);
        let connection = self.lock()?;
        let mut statement = connection
            .prepare(
                "SELECT id, uid, title, url, media_type, pipeline_status, failure_reason,
                        created_at, last_seen_at, l1_id, l2_id
                 FROM entries
                 WHERE pipeline_status IN ('pending','processing','failed')
                   AND (?1 IS NULL OR created_at < ?1 OR (created_at = ?1 AND id < ?2))
                 ORDER BY created_at DESC, id DESC
                 LIMIT ?3",
            )
            .map_err(StorageError::Inbox)?;

        let mut rows = statement
            .query(params![
                cursor.map(|cursor| cursor.created_at),
                cursor.map(|cursor| cursor.id),
                (limit + 1) as i64
            ])
            .map_err(StorageError::Inbox)?;

        let mut entries = Vec::with_capacity(limit);
        while let Some(row) = rows.next().map_err(StorageError::Inbox)? {
            entries.push(InboxEntry {
                id: row.get(0).map_err(StorageError::Inbox)?,
                uid: row.get(1).map_err(StorageError::Inbox)?,
                title: row.get(2).map_err(StorageError::Inbox)?,
                url: row.get(3).map_err(StorageError::Inbox)?,
                media_type: row.get(4).map_err(StorageError::Inbox)?,
                pipeline_status: row.get(5).map_err(StorageError::Inbox)?,
                failure_reason: row.get(6).map_err(StorageError::Inbox)?,
                created_at: row.get(7).map_err(StorageError::Inbox)?,
                last_seen_at: row.get(8).map_err(StorageError::Inbox)?,
                l1_id: row.get(9).map_err(StorageError::Inbox)?,
                l2_id: row.get(10).map_err(StorageError::Inbox)?,
            });
        }
        drop(rows);
        drop(statement);

        let next_cursor = if entries.len() > limit {
            entries.truncate(limit);
            entries.last().map(|entry| InboxCursor {
                created_at: entry.created_at,
                id: entry.id,
            })
        } else {
            None
        };

        Ok(InboxPage {
            entries,
            next_cursor,
        })
    }

    /// 测试观察口：设置表的全部键值（断言密钥明文永远不会出现在 DB 里）。
    #[cfg(test)]
    pub(crate) fn settings_snapshot(&self) -> Vec<(String, String)> {
        let Ok(connection) = self.lock() else {
            return Vec::new();
        };
        let Ok(mut statement) = connection.prepare("SELECT key, value_json FROM settings") else {
            return Vec::new();
        };
        let rows = statement.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        });
        match rows {
            Ok(rows) => rows.filter_map(Result::ok).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Connection>, StorageError> {
        self.connection.lock().map_err(|_| StorageError::Poisoned)
    }
}

/// 命中既存条目：只更新 last_seen_at（不写 updated_at），返回最近出现时间。
fn touch_entry_last_seen(
    connection: &Connection,
    entry_id: i64,
    current: i64,
    now_ms: i64,
) -> Result<i64, rusqlite::Error> {
    let last_seen_at = current.max(now_ms);
    connection.execute(
        "UPDATE entries SET last_seen_at = ?1 WHERE id = ?2",
        params![last_seen_at, entry_id],
    )?;
    Ok(last_seen_at)
}

fn duplicate_outcome(existing: ExistingEntry, last_seen_at: i64) -> IngestOutcome {
    IngestOutcome {
        entry_id: existing.id,
        duplicate: true,
        stage: None,
        pipeline_status: existing.pipeline_status,
        title: existing.title,
        created_at: existing.created_at,
        last_seen_at,
    }
}

struct ExistingEntry {
    id: i64,
    title: String,
    pipeline_status: String,
    created_at: i64,
    last_seen_at: i64,
}

fn find_by_source(
    connection: &Connection,
    source_type: &str,
    source_ref: &str,
) -> Result<Option<ExistingEntry>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT e.id, e.title, e.pipeline_status, e.created_at, e.last_seen_at
             FROM entries e
             JOIN entry_sources s ON s.entry_id = e.id
             WHERE s.source_type = ?1 AND s.source_ref = ?2",
            params![source_type, source_ref],
            existing_entry_from_row,
        )
        .optional()
}

fn find_by_url_hash(
    connection: &Connection,
    hash: &[u8],
) -> Result<Option<ExistingEntry>, rusqlite::Error> {
    connection
        .query_row(
            "SELECT id, title, pipeline_status, created_at, last_seen_at
             FROM entries WHERE url_hash = ?1",
            params![hash],
            existing_entry_from_row,
        )
        .optional()
}

fn existing_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ExistingEntry> {
    Ok(ExistingEntry {
        id: row.get(0)?,
        title: row.get(1)?,
        pipeline_status: row.get(2)?,
        created_at: row.get(3)?,
        last_seen_at: row.get(4)?,
    })
}

/// v1 的表是否齐全（旧开发库探测；迁移后仍缺表说明 user_version 与 schema 不一致）。
fn schema_is_complete(connection: &Connection) -> Result<bool, rusqlite::Error> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_master
         WHERE type = 'table' AND name IN ('entries', 'pipeline_jobs', 'fts_entries')",
        [],
        |row| row.get(0),
    )?;
    Ok(count == 3)
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
    use crate::ingest::{plan_manual, PipelineStage};

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!(
                "aether-storage-test-{}-{}",
                std::process::id(),
                fastrand::u64(..)
            ))
            .join("aether.db")
    }

    fn scalar_i64(db: &Database, sql: &str, params: impl rusqlite::Params) -> i64 {
        let connection = db.connection.lock().unwrap();
        connection.query_row(sql, params, |row| row.get(0)).unwrap()
    }

    fn search_ids(db: &Database, plan: crate::search::SearchPlan) -> Vec<i64> {
        let connection = db.connection.lock().unwrap();
        let mut ids = Vec::new();
        match plan {
            crate::search::SearchPlan::Match(expression) => {
                let mut statement = connection
                    .prepare("SELECT rowid FROM fts_entries WHERE fts_entries MATCH ?1")
                    .unwrap();
                let rows = statement
                    .query_map([expression], |row| row.get::<_, i64>(0))
                    .unwrap();
                for row in rows {
                    ids.push(row.unwrap());
                }
            }
            crate::search::SearchPlan::Like(pattern) => {
                let mut statement = connection
                    .prepare("SELECT id FROM entries WHERE title LIKE ?1 ESCAPE '\\'")
                    .unwrap();
                let rows = statement
                    .query_map([pattern], |row| row.get::<_, i64>(0))
                    .unwrap();
                for row in rows {
                    ids.push(row.unwrap());
                }
            }
        }
        ids.sort_unstable();
        ids
    }

    fn seed_text(db: &Database, text: &str, title: &str, now_ms: i64) -> i64 {
        let mut plan = plan_manual(text, now_ms).unwrap();
        plan.title = title.to_string();
        plan.excerpt = Some(text.to_string());
        plan.body_md = Some(text.to_string());
        db.ingest(&plan, now_ms).unwrap().entry_id
    }

    #[test]
    fn migration_v1_creates_every_table_index_trigger_and_user_version() {
        let db = Database::open_in_memory().unwrap();
        let connection = db.connection.lock().unwrap();

        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(user_version, 1);

        for table in [
            "entries",
            "tags",
            "tag_aliases",
            "entry_tags",
            "entry_sources",
            "corrections",
            "pipeline_jobs",
            "settings",
            "meta",
            "fts_entries",
        ] {
            let count: i64 = connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "缺少表 {table}");
        }

        for index in [
            "ux_tags_normalized_key",
            "ix_tag_aliases_tag",
            "ix_entry_tags_tag",
            "ux_entry_sources_ref",
            "ix_entry_sources_entry",
            "ix_corrections_entry",
            "ix_corrections_created",
            "ix_pipeline_jobs_ready",
            "ix_pipeline_jobs_lease",
            "ux_entries_url_hash",
            "ix_entries_l1_l2",
            "ix_entries_pipeline",
            "ix_entries_lifecycle_created",
            "ix_entries_trashed",
        ] {
            let count: i64 = connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
                    [index],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "缺少索引 {index}");
        }

        for trigger in ["entries_ai", "entries_ad", "entries_au"] {
            let count: i64 = connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type = 'trigger' AND name = ?1",
                    [trigger],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "缺少触发器 {trigger}");
        }
    }

    #[test]
    fn connection_pragmas_match_the_spec() {
        let db_path = temp_db_path();
        let db = Database::open(&db_path).unwrap();
        let connection = db.connection.lock().unwrap();

        let journal: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal.to_ascii_lowercase(), "wal");

        let synchronous: i64 = connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 1, "synchronous 应为 NORMAL(1)");

        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(foreign_keys, 1);

        let busy_timeout: i64 = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .unwrap();
        assert_eq!(busy_timeout, 5000);

        drop(connection);
        drop(db);
        let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn fts_index_tracks_insert_update_and_delete() {
        let db = Database::open_in_memory().unwrap();
        let entry_id = seed_text(&db, "Rust 异步编程指南", "Rust 异步编程指南", 1_000);

        assert_eq!(
            search_ids(&db, crate::search::plan("异步编程").unwrap()),
            vec![entry_id]
        );

        {
            let connection = db.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE entries SET title = 'Python 入门', excerpt = 'Python 入门' WHERE id = ?1",
                    [entry_id],
                )
                .unwrap();
        }
        assert!(search_ids(&db, crate::search::plan("异步编程").unwrap()).is_empty());
        assert_eq!(
            search_ids(&db, crate::search::plan("Python").unwrap()),
            vec![entry_id]
        );

        {
            let connection = db.connection.lock().unwrap();
            connection
                .execute("DELETE FROM entries WHERE id = ?1", [entry_id])
                .unwrap();
        }
        assert!(search_ids(&db, crate::search::plan("Python").unwrap()).is_empty());
    }

    #[test]
    fn short_queries_use_like_and_escape_wildcards() {
        let db = Database::open_in_memory().unwrap();
        let rust = seed_text(&db, "Rust 安全指南", "Rust 安全指南", 1_000);
        let percent = seed_text(&db, "100% 完成", "100% 完成", 2_000);

        // 2 个字符：LIKE 前缀兜底。
        assert_eq!(
            search_ids(&db, crate::search::plan("Ru").unwrap()),
            vec![rust]
        );
        // 只有字面量 % 会命中，通配符语义被转义掉。
        assert_eq!(
            search_ids(&db, crate::search::plan("%").unwrap()),
            vec![percent]
        );
    }

    #[test]
    fn fts_syntax_injection_does_not_change_semantics_or_error() {
        let db = Database::open_in_memory().unwrap();
        let rust = seed_text(&db, "Rust 安全指南", "Rust 安全指南", 1_000);
        let python = seed_text(&db, "Python 入门教程", "Python 入门教程", 2_000);
        let quoted = seed_text(&db, "他说你好世界", "他说你好世界", 3_000);

        // OR 被当作普通 AND 词：没有条目同时含 Rust 与 Python → 0 行，且不报错。
        assert!(search_ids(&db, crate::search::plan("Rust OR Python").unwrap()).is_empty());
        // 普通多词查询仍按 AND 命中。
        assert_eq!(
            search_ids(&db, crate::search::plan("Python 入门教程").unwrap()),
            vec![python]
        );
        assert_eq!(
            search_ids(&db, crate::search::plan("他说你好").unwrap()),
            vec![quoted]
        );
        // 前缀星号、减号、括号、列过滤都只当普通字符/分段，不改变语义。
        for raw in ["NEAR(Rust Python)", "title:Rust", "-Rust"] {
            let plan = crate::search::plan(raw).expect("应产生检索计划");
            let _ = search_ids(&db, plan);
        }
        assert_eq!(
            search_ids(&db, crate::search::plan("Rust*").unwrap()),
            vec![rust]
        );
        // 用户输入的引号按字面量处理（段内双写），不报错。
        let plan = crate::search::plan("他说\"你好").expect("应产生检索计划");
        let _ = search_ids(&db, plan);
    }

    #[test]
    fn ingest_writes_entry_source_and_job_in_one_transaction() {
        let db = Database::open_in_memory().unwrap();
        let plan = plan_manual("随手记一条", 1_000).unwrap();
        let outcome = db.ingest(&plan, 1_000).unwrap();

        assert!(!outcome.duplicate);
        assert_eq!(outcome.stage, Some(PipelineStage::Classify));
        assert_eq!(outcome.pipeline_status, "pending");
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entries", []), 1);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entry_sources", []), 1);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM pipeline_jobs", []), 1);
        assert_eq!(
            scalar_i64(
                &db,
                "SELECT attempts FROM pipeline_jobs WHERE entry_id = ?1",
                [outcome.entry_id]
            ),
            0
        );
    }

    #[test]
    fn a_failed_insert_rolls_back_entry_source_and_job() {
        let db = Database::open_in_memory().unwrap();
        let first = plan_manual("第一条", 1_000).unwrap();
        let first_outcome = db.ingest(&first, 1_000).unwrap();

        // 复用已存在的 uid：entries.uid UNIQUE 失败 → 整个事务回滚。
        let mut second = plan_manual("第二条", 2_000).unwrap();
        second.uid = first.uid.clone();
        assert!(db.ingest(&second, 2_000).is_err());

        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entries", []), 1);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entry_sources", []), 1);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM pipeline_jobs", []), 1);
        assert_eq!(first_outcome.entry_id, 1);
    }

    #[test]
    fn source_track_dedupes_and_only_touches_last_seen() {
        let db = Database::open_in_memory().unwrap();
        let first = plan_manual("第一条消息", 1_000).unwrap();
        let mut first = first;
        first.source_type = "webhook:telegram".to_string();
        first.source_ref = "msg-1".to_string();
        let outcome = db.ingest(&first, 1_000).unwrap();
        assert!(!outcome.duplicate);

        // 同一来源记录再次投递：不新建条目/来源/job，只更新 last_seen_at。
        let mut second = plan_manual("第一条消息（编辑过）", 2_000).unwrap();
        second.source_type = "webhook:telegram".to_string();
        second.source_ref = "msg-1".to_string();
        let duplicate = db.ingest(&second, 2_000).unwrap();

        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry_id, outcome.entry_id);
        assert_eq!(duplicate.stage, None);
        assert_eq!(duplicate.last_seen_at, 2_000);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entries", []), 1);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entry_sources", []), 1);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM pipeline_jobs", []), 1);
        assert_eq!(
            scalar_i64(
                &db,
                "SELECT last_seen_at FROM entries WHERE id = ?1",
                [outcome.entry_id]
            ),
            2_000
        );
        assert_eq!(
            scalar_i64(&db, "SELECT last_seen_at FROM entry_sources", []),
            2_000
        );
    }

    #[test]
    fn canonical_url_track_dedupes_and_records_the_new_delivery_source() {
        let db = Database::open_in_memory().unwrap();
        let first = plan_manual("https://example.com/read?utm_source=a&b=2&a=1", 1_000).unwrap();
        let outcome = db.ingest(&first, 1_000).unwrap();
        assert!(!outcome.duplicate);
        assert_eq!(outcome.stage, Some(PipelineStage::Fetch));

        let second = plan_manual("https://EXAMPLE.com/read/?b=2&a=1#section", 2_000).unwrap();
        let duplicate = db.ingest(&second, 2_000).unwrap();

        assert!(duplicate.duplicate);
        assert_eq!(duplicate.entry_id, outcome.entry_id);
        assert_eq!(duplicate.stage, None);
        assert_eq!(duplicate.last_seen_at, 2_000);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entries", []), 1);
        // 两次投递各留一条来源记录，都指向同一条目。
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entry_sources", []), 2);
        assert_eq!(
            scalar_i64(
                &db,
                "SELECT count(*) FROM entry_sources WHERE entry_id = ?1",
                [outcome.entry_id]
            ),
            2
        );
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM pipeline_jobs", []), 1);
    }

    #[test]
    fn identical_plain_text_is_not_a_duplicate_without_a_url() {
        let db = Database::open_in_memory().unwrap();
        db.ingest(&plan_manual("同一条随手记", 1_000).unwrap(), 1_000)
            .unwrap();
        let second = db
            .ingest(&plan_manual("同一条随手记", 2_000).unwrap(), 2_000)
            .unwrap();
        assert!(!second.duplicate);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM entries", []), 2);
        assert_eq!(scalar_i64(&db, "SELECT count(*) FROM pipeline_jobs", []), 2);
    }

    #[test]
    fn inbox_page_is_newest_first_keyset_paginated_at_fifty() {
        let db = Database::open_in_memory().unwrap();
        let mut ids = Vec::new();
        for index in 0..55 {
            let text = format!("收件箱条目 {index}");
            let outcome = db
                .ingest(
                    &plan_manual(&text, 1_000 + index as i64).unwrap(),
                    1_000 + index as i64,
                )
                .unwrap();
            ids.push(outcome.entry_id);
        }
        // 分类完成的条目离开收件箱；处理中与失败仍在。
        {
            let connection = db.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE entries SET pipeline_status = 'classified' WHERE id = ?1",
                    [ids[0]],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE entries SET pipeline_status = 'processing' WHERE id = ?1",
                    [ids[53]],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE entries SET pipeline_status = 'failed', failure_reason = 'fetch_failed' WHERE id = ?1",
                    [ids[52]],
                )
                .unwrap();
        }

        let first_page = db.inbox_page(None, 50).unwrap();
        assert_eq!(first_page.entries.len(), 50);
        assert!(first_page.next_cursor.is_some());
        let first_ids: Vec<i64> = first_page.entries.iter().map(|entry| entry.id).collect();
        // created_at 倒序：最后创建的 54 号在最前，且不含已分类的 0 号。
        assert_eq!(first_ids[0], ids[54]);
        assert_eq!(first_ids[49], ids[5]);
        assert!(!first_ids.contains(&ids[0]));
        assert!(first_page
            .entries
            .iter()
            .any(|entry| entry.pipeline_status == "processing"));
        assert!(first_page
            .entries
            .iter()
            .any(|entry| entry.pipeline_status == "failed"
                && entry.failure_reason.as_deref() == Some("fetch_failed")));

        let second_page = db.inbox_page(first_page.next_cursor, 50).unwrap();
        let second_ids: Vec<i64> = second_page.entries.iter().map(|entry| entry.id).collect();
        // 剩余 4 条（1 processing、2 failed、3/4 pending）。
        assert_eq!(second_ids, vec![ids[4], ids[3], ids[2], ids[1]]);
        assert_eq!(second_page.next_cursor, None);

        // 比最旧一行还旧的游标 → 空页（游标不会重复返回最后一行）。
        let past_end = db
            .inbox_page(
                Some(InboxCursor {
                    created_at: 0,
                    id: 0,
                }),
                50,
            )
            .unwrap();
        assert!(past_end.entries.is_empty());
        assert_eq!(past_end.next_cursor, None);
    }

    #[test]
    fn settings_round_trip_through_json_and_delete() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(
            db.setting_get::<String>("classifier.api_key_ref").unwrap(),
            None
        );

        db.setting_set(
            "classifier.api_key_ref",
            &"keychain://aetherhud/classifier.api_key".to_string(),
            1_000,
        )
        .unwrap();
        assert_eq!(
            db.setting_get::<String>("classifier.api_key_ref")
                .unwrap()
                .as_deref(),
            Some("keychain://aetherhud/classifier.api_key")
        );

        // 覆盖写仍只有一行（key 是主键）。
        db.setting_set(
            "classifier.api_key_ref",
            &"keychain://aetherhud/classifier.api_key".to_string(),
            2_000,
        )
        .unwrap();
        assert_eq!(
            scalar_i64(
                &db,
                "SELECT count(*) FROM settings WHERE key = ?1",
                ["classifier.api_key_ref"]
            ),
            1
        );

        assert!(db.setting_delete("classifier.api_key_ref").unwrap());
        assert_eq!(
            db.setting_get::<String>("classifier.api_key_ref").unwrap(),
            None
        );
        assert!(!db.setting_delete("classifier.api_key_ref").unwrap());
    }

    #[test]
    fn a_stale_development_database_gets_a_clear_error() {
        let db_path = temp_db_path();
        std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        {
            let connection = Connection::open(&db_path).unwrap();
            connection
                .execute_batch(
                    "PRAGMA user_version = 1;
                     CREATE TABLE meta (
                       key TEXT PRIMARY KEY,
                       value TEXT NOT NULL,
                       updated_at INTEGER NOT NULL
                     ) WITHOUT ROWID;",
                )
                .unwrap();
        }

        match Database::open(&db_path) {
            Err(StorageError::StaleDevelopmentDatabase { path }) => {
                assert_eq!(path, db_path);
            }
            other => panic!(
                "旧的开发库应给出 StaleDevelopmentDatabase，实际：{}",
                match other {
                    Ok(_) => "Ok(Database)".to_string(),
                    Err(error) => error.to_string(),
                }
            ),
        }
        let _ = std::fs::remove_dir_all(db_path.parent().unwrap());
    }

    #[test]
    fn migrations_are_valid_for_a_fresh_database() {
        migrations().validate().unwrap();
    }
}
