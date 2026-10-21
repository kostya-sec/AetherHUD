//! 纠偏候选生成（#14 §6.1 / #18 §8.4）。
//!
//! corrections JOIN entries → 候选 JSONL：
//! - 只取分类维度（l1/l2），标签增删与媒体类型变更不是分类错例；
//! - 按 entry_id 去重（一次「采纳建议」会写 L1/L2 两行）；
//! - entry_id 为 NULL（条目硬删）没有文本，跳过并计数。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;

use crate::util;

#[derive(Debug, thiserror::Error)]
pub enum CandidatesError {
    #[error("打开数据库失败（{path}）：{source}")]
    Db {
        path: String,
        #[source]
        source: rusqlite::Error,
    },
    #[error("查询纠偏记录失败：{0}")]
    Query(String),
    #[error("写入候选文件失败（{path}）：{source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct CorrectionSummary {
    pub id: i64,
    pub dimension: String,
    pub trigger: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub taxonomy_version: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidateReview {
    pub status: String,
    pub verdict: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Candidate {
    pub entry_id: i64,
    pub entry_uid: String,
    pub title: String,
    pub url: Option<String>,
    pub media_type: String,
    pub body_excerpt: String,
    pub taxonomy_version: Option<String>,
    pub corrected_at: i64,
    /// dimension（l1/l2）→ 模型当时的输出快照
    pub before: BTreeMap<String, Value>,
    /// dimension（l1/l2）→ 用户金标快照
    pub after: BTreeMap<String, Value>,
    pub corrections: Vec<CorrectionSummary>,
    pub review: CandidateReview,
}

#[derive(Debug, Clone, Serialize)]
pub struct CandidatesSummary {
    pub generated_at: String,
    pub db_path: String,
    pub out_file: String,
    pub summary_file: String,
    pub total_correction_rows: usize,
    pub hard_deleted_rows_skipped: usize,
    pub excluded_non_classification_rows: usize,
    pub skipped_missing_entry_rows: usize,
    pub duplicate_rows_merged: usize,
    pub entries_with_candidates: usize,
    pub emitted_candidates: usize,
    pub rows_by_dimension: BTreeMap<String, usize>,
    pub rows_by_trigger: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
struct CorrectionRow {
    id: i64,
    entry_id: i64,
    dimension: String,
    before: Option<String>,
    after: Option<String>,
    trigger: String,
    taxonomy_version: Option<String>,
    created_at: i64,
}

#[derive(Debug)]
struct EntryRow {
    uid: String,
    title: String,
    url: Option<String>,
    media_type: String,
    body_md: Option<String>,
}

#[derive(Debug, Default)]
struct EntryAccumulator {
    entry: Option<EntryRow>,
    corrections: Vec<CorrectionRow>,
}

fn parse_snapshot(raw: Option<&str>) -> Option<Value> {
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    match serde_json::from_str::<Value>(trimmed) {
        Ok(value) => Some(value),
        Err(_) => Some(Value::String(trimmed.to_string())),
    }
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        s.chars().take(max).collect()
    }
}

pub fn generate_candidates(
    db_path: &Path,
    out_dir: &Path,
) -> Result<(CandidatesSummary, Vec<Candidate>), CandidatesError> {
    let connection = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|source| CandidatesError::Db {
            path: db_path.display().to_string(),
            source,
        })?;
    let mut statement = connection
        .prepare(
            "SELECT c.id, c.entry_id, c.dimension, c.before_value, c.after_value, c.trigger, c.taxonomy_version, c.created_at, \
                    e.uid, e.title, e.url, e.media_type, e.body_md \
             FROM corrections c LEFT JOIN entries e ON e.id = c.entry_id \
             ORDER BY c.created_at ASC, c.id ASC",
        )
        .map_err(|e| CandidatesError::Query(e.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            let entry_id: Option<i64> = row.get(1)?;
            Ok((
                CorrectionRow {
                    id: row.get(0)?,
                    entry_id: entry_id.unwrap_or_default(),
                    dimension: row.get(2)?,
                    before: row.get(3)?,
                    after: row.get(4)?,
                    trigger: row.get(5)?,
                    taxonomy_version: row.get(6)?,
                    created_at: row.get(7)?,
                },
                entry_id.is_none(),
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<String>>(9)?,
                row.get::<_, Option<String>>(10)?,
                row.get::<_, Option<String>>(11)?,
                row.get::<_, Option<String>>(12)?,
            ))
        })
        .map_err(|e| CandidatesError::Query(e.to_string()))?;

    let mut summary = CandidatesSummary {
        generated_at: util::now_rfc3339(),
        db_path: db_path.display().to_string(),
        out_file: String::new(),
        summary_file: String::new(),
        total_correction_rows: 0,
        hard_deleted_rows_skipped: 0,
        excluded_non_classification_rows: 0,
        skipped_missing_entry_rows: 0,
        duplicate_rows_merged: 0,
        entries_with_candidates: 0,
        emitted_candidates: 0,
        rows_by_dimension: BTreeMap::new(),
        rows_by_trigger: BTreeMap::new(),
    };
    let mut groups: BTreeMap<i64, EntryAccumulator> = BTreeMap::new();
    for row in rows {
        let (correction, hard_deleted, uid, title, url, media_type, body_md) =
            row.map_err(|e| CandidatesError::Query(e.to_string()))?;
        summary.total_correction_rows += 1;
        *summary
            .rows_by_dimension
            .entry(correction.dimension.clone())
            .or_default() += 1;
        *summary
            .rows_by_trigger
            .entry(correction.trigger.clone())
            .or_default() += 1;
        if hard_deleted {
            summary.hard_deleted_rows_skipped += 1;
            continue;
        }
        if !matches!(correction.dimension.as_str(), "l1" | "l2") {
            summary.excluded_non_classification_rows += 1;
            continue;
        }
        let Some(uid) = uid.filter(|s| !s.trim().is_empty()) else {
            summary.skipped_missing_entry_rows += 1;
            continue;
        };
        let accumulator = groups.entry(correction.entry_id).or_default();
        if accumulator.entry.is_none() {
            accumulator.entry = Some(EntryRow {
                uid,
                title: title.unwrap_or_default(),
                url,
                media_type: media_type.unwrap_or_else(|| "link".to_string()),
                body_md,
            });
        }
        accumulator.corrections.push(correction);
    }

    let mut candidates = Vec::new();
    for (entry_id, accumulator) in groups {
        let Some(entry) = accumulator.entry else {
            continue;
        };
        let mut corrections = accumulator.corrections;
        corrections.sort_by_key(|c| (c.created_at, c.id));
        summary.duplicate_rows_merged += corrections.len().saturating_sub(1);
        let mut before: BTreeMap<String, Value> = BTreeMap::new();
        let mut after: BTreeMap<String, Value> = BTreeMap::new();
        let mut taxonomy_version: Option<String> = None;
        let mut corrected_at = 0i64;
        let mut summaries = Vec::with_capacity(corrections.len());
        for correction in &corrections {
            corrected_at = corrected_at.max(correction.created_at);
            if let Some(raw) = &correction.before {
                if let Some(value) = parse_snapshot(Some(raw)) {
                    before.insert(correction.dimension.clone(), value);
                }
            }
            if let Some(raw) = &correction.after {
                if let Some(value) = parse_snapshot(Some(raw)) {
                    after.insert(correction.dimension.clone(), value);
                }
            }
            if correction.taxonomy_version.is_some() {
                taxonomy_version = correction.taxonomy_version.clone();
            }
            summaries.push(CorrectionSummary {
                id: correction.id,
                dimension: correction.dimension.clone(),
                trigger: correction.trigger.clone(),
                before: parse_snapshot(correction.before.as_deref()),
                after: parse_snapshot(correction.after.as_deref()),
                taxonomy_version: correction.taxonomy_version.clone(),
                created_at: correction.created_at,
            });
        }
        candidates.push(Candidate {
            entry_id,
            entry_uid: entry.uid,
            title: entry.title,
            url: entry.url,
            media_type: entry.media_type,
            body_excerpt: truncate_chars(entry.body_md.as_deref().unwrap_or(""), 1500),
            taxonomy_version,
            corrected_at,
            before,
            after,
            corrections: summaries,
            review: CandidateReview {
                status: "pending".to_string(),
                verdict: None,
                note: None,
            },
        });
    }
    candidates.sort_by_key(|c| (c.corrected_at, c.entry_id));
    summary.entries_with_candidates = candidates.len();
    summary.emitted_candidates = candidates.len();

    util::ensure_dir(out_dir).map_err(|source| CandidatesError::Write {
        path: out_dir.display().to_string(),
        source,
    })?;
    let timestamp = util::file_timestamp();
    let out_file: PathBuf = out_dir.join(format!("candidates-{timestamp}.jsonl"));
    let summary_file: PathBuf = out_dir.join(format!("candidates-{timestamp}.summary.json"));
    let mut jsonl = String::new();
    for candidate in &candidates {
        jsonl.push_str(&serde_json::to_string(candidate).unwrap_or_default());
        jsonl.push('\n');
    }
    util::write_new(&out_file, &jsonl).map_err(|source| CandidatesError::Write {
        path: out_file.display().to_string(),
        source,
    })?;
    summary.out_file = out_file.display().to_string();
    summary.summary_file = summary_file.display().to_string();
    let summary_text = serde_json::to_string_pretty(&summary).unwrap_or_default();
    util::write_new(&summary_file, &summary_text).map_err(|source| CandidatesError::Write {
        path: summary_file.display().to_string(),
        source,
    })?;
    Ok((summary, candidates))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_db() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aether-candidates-test-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("aether.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE entries (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                uid TEXT NOT NULL UNIQUE,
                title TEXT NOT NULL,
                url TEXT,
                media_type TEXT NOT NULL DEFAULT 'link',
                body_md TEXT,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
             );
             CREATE TABLE corrections (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entry_id INTEGER REFERENCES entries(id) ON DELETE SET NULL,
                dimension TEXT NOT NULL,
                before_value TEXT,
                after_value TEXT,
                trigger TEXT NOT NULL,
                taxonomy_version TEXT,
                created_at INTEGER NOT NULL
             );",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entries (uid,title,url,media_type,body_md,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            rusqlite::params![
                "01ENTRY1",
                "标题一",
                "https://example.com/a",
                "link",
                "长正文".repeat(800),
                1000i64,
                1000i64
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO entries (uid,title,url,media_type,body_md,created_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            rusqlite::params!["01ENTRY2", "标题二", "", "article", "正文二", 1000i64, 1000i64],
        )
        .unwrap();
        // 采纳建议：同一 entry 写 L1、L2 两行 → 一个候选
        conn.execute(
            "INSERT INTO corrections (entry_id,dimension,before_value,after_value,trigger,taxonomy_version,created_at) VALUES (?1,'l1',?2,?3,'manual_edit',?4,?5)",
            rusqlite::params![
                1i64,
                r#"{"id":"l1_tech","name":"03 技术与工程"}"#,
                r#"{"id":"l1_biz","name":"04 商业与认知"}"#,
                "1.1.0",
                2000i64
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO corrections (entry_id,dimension,before_value,after_value,trigger,taxonomy_version,created_at) VALUES (?1,'l2',?2,?3,'manual_edit',?4,?5)",
            rusqlite::params![
                1i64,
                r#"{"id":"l2_coding","name":"编程与低代码"}"#,
                r#"{"id":"l2_insight","name":"行业观察与研报"}"#,
                "1.1.0",
                2001i64
            ],
        )
        .unwrap();
        // 标签增删：非分类维度，排除
        conn.execute(
            "INSERT INTO corrections (entry_id,dimension,before_value,after_value,trigger,created_at) VALUES (?1,'tag_add',NULL,?2,'manual_edit',?3)",
            rusqlite::params![2i64, r#"{"name":"Cursor"}"#, 2100i64],
        )
        .unwrap();
        // 硬删条目：entry_id NULL，跳过并计数
        conn.execute(
            "INSERT INTO corrections (entry_id,dimension,before_value,after_value,trigger,created_at) VALUES (NULL,'l1',?1,?2,'manual_edit',?3)",
            rusqlite::params![
                r#"{"id":"l1_tech"}"#,
                r#"{"id":"l1_biz"}"#,
                2200i64
            ],
        )
        .unwrap();
        // 第二条候选：仅 L2 纠偏
        conn.execute(
            "INSERT INTO corrections (entry_id,dimension,before_value,after_value,trigger,created_at) VALUES (?1,'l2',?2,?3,'manual_edit',?4)",
            rusqlite::params![
                2i64,
                r#"{"id":"l2_writing"}"#,
                r#"{"id":"l2_km"}"#,
                2300i64
            ],
        )
        .unwrap();
        path
    }

    #[test]
    fn candidates_dedupe_skip_and_count() {
        let db = fixture_db();
        let out = db.parent().unwrap().join("candidates");
        let (summary, candidates) = generate_candidates(&db, &out).unwrap();
        assert_eq!(summary.total_correction_rows, 5);
        assert_eq!(summary.hard_deleted_rows_skipped, 1);
        assert_eq!(summary.excluded_non_classification_rows, 1);
        assert_eq!(summary.duplicate_rows_merged, 1);
        assert_eq!(summary.entries_with_candidates, 2);
        assert_eq!(summary.emitted_candidates, 2);
        assert_eq!(candidates.len(), 2);
        let first = &candidates[0];
        assert_eq!(first.entry_uid, "01ENTRY1");
        assert_eq!(first.body_excerpt.chars().count(), 1500);
        assert_eq!(first.after["l1"]["id"], "l1_biz");
        assert_eq!(first.after["l2"]["id"], "l2_insight");
        assert_eq!(first.before["l2"]["id"], "l2_coding");
        assert_eq!(first.taxonomy_version.as_deref(), Some("1.1.0"));
        assert!(first.summary_has_both_dimensions());
        let jsonl = std::fs::read_to_string(&summary.out_file).unwrap();
        assert_eq!(jsonl.lines().count(), 2);
        assert!(summary.summary_file.ends_with(".summary.json"));
    }

    impl Candidate {
        fn summary_has_both_dimensions(&self) -> bool {
            self.corrections.iter().any(|c| c.dimension == "l1")
                && self.corrections.iter().any(|c| c.dimension == "l2")
        }
    }
}
