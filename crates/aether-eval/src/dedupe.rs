//! few-shot 池与评测集样本级查重（#14 §6.2 / #19 §9）。
//!
//! 同一内容不得同时出现在 few-shot 池与主评测集（scored set）；
//! 边界集不参与门线，重叠只作警告（#19 的弃权示例本身取自边界集）。

use std::path::Path;

use serde::Serialize;

use aether_classifier::config::{load_few_shots, ConfigError};
use aether_classifier::prompt::FewShotExample;

use crate::eval_set::{load_eval_set, EvalItem, EvalSetError};
use crate::util;

#[derive(Debug, thiserror::Error)]
pub enum DedupeError {
    #[error("读取 few-shot 配置失败：{0}")]
    FewShot(#[from] ConfigError),
    #[error("读取评测集失败：{0}")]
    EvalSet(#[from] EvalSetError),
}

#[derive(Debug, Clone, Serialize)]
pub struct DedupeMatch {
    pub fewshot_index: usize,
    pub fewshot_entry_id: Option<String>,
    pub fewshot_title: String,
    pub matched_line: usize,
    pub matched_entry_id: String,
    pub matched_title: String,
    pub matched_by: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DedupeReport {
    pub fewshot_file: String,
    pub main_set: String,
    pub boundary_set: Option<String>,
    pub fewshot_count: usize,
    pub fewshots_without_entry_id: usize,
    pub main_overlaps: Vec<DedupeMatch>,
    pub boundary_overlaps: Vec<DedupeMatch>,
    pub warnings: Vec<String>,
}

impl DedupeReport {
    pub fn has_main_overlaps(&self) -> bool {
        !self.main_overlaps.is_empty()
    }

    pub fn exit_code(&self) -> i32 {
        if self.has_main_overlaps() {
            1
        } else {
            0
        }
    }
}

#[derive(Debug, Clone)]
struct Fingerprint {
    entry_id: Option<String>,
    title: String,
    content_hash: String,
}

fn collapse_whitespace(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut last_space = false;
    for ch in s.trim().chars() {
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out
}

fn canonical_url(url: &str) -> String {
    let trimmed = url.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return String::new();
    }
    let mut rest = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(&trimmed)
        .to_string();
    if let Some(stripped) = rest.strip_prefix("www.") {
        rest = stripped.to_string();
    }
    if let Some(pos) = rest.find(['?', '#']) {
        rest.truncate(pos);
    }
    rest.trim_end_matches('/').to_string()
}

fn fingerprint(entry_id: Option<&str>, title: &str, url: Option<&str>) -> Fingerprint {
    let title = collapse_whitespace(title);
    let url = canonical_url(url.unwrap_or(""));
    let hash_input = format!("{title}\n{url}");
    Fingerprint {
        entry_id: entry_id
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        title,
        content_hash: util::sha256_hex(hash_input.as_bytes()),
    }
}

fn match_fingerprint(few: &Fingerprint, item: &Fingerprint) -> Option<&'static str> {
    if let (Some(a), Some(b)) = (&few.entry_id, &item.entry_id) {
        if a == b {
            return Some("entry_id");
        }
    }
    if !few.content_hash.is_empty() && few.content_hash == item.content_hash {
        return Some("content_hash");
    }
    if !few.title.is_empty() && few.title == item.title {
        return Some("title");
    }
    None
}

fn item_fingerprint(item: &EvalItem) -> Fingerprint {
    fingerprint(
        Some(item.entry_id.as_str()),
        &item.title,
        Some(item.url.as_str()),
    )
}

fn fewshot_fingerprint(example: &FewShotExample) -> Fingerprint {
    fingerprint(
        example.entry_id.as_deref(),
        &example.input.title,
        example.input.url.as_deref(),
    )
}

pub fn check_dedupe(
    fewshot_path: &Path,
    main_set_path: &Path,
    boundary_path: Option<&Path>,
) -> Result<DedupeReport, DedupeError> {
    let few_shots = load_few_shots(fewshot_path)?;
    let main = load_eval_set(main_set_path)?;
    let boundary = match boundary_path {
        Some(path) => Some(load_eval_set(path)?),
        None => None,
    };
    let few_prints: Vec<Fingerprint> = few_shots.iter().map(fewshot_fingerprint).collect();

    let mut report = DedupeReport {
        fewshot_file: fewshot_path.display().to_string(),
        main_set: main_set_path.display().to_string(),
        boundary_set: boundary_path.map(|p| p.display().to_string()),
        fewshot_count: few_shots.len(),
        fewshots_without_entry_id: few_shots
            .iter()
            .filter(|ex| {
                ex.entry_id
                    .as_deref()
                    .map(str::trim)
                    .unwrap_or("")
                    .is_empty()
            })
            .count(),
        main_overlaps: Vec::new(),
        boundary_overlaps: Vec::new(),
        warnings: Vec::new(),
    };
    if report.fewshots_without_entry_id > 0 {
        report.warnings.push(format!(
            "{} 条 few-shot 未记录 entry_id，仅按内容哈希 / 标题查重",
            report.fewshots_without_entry_id
        ));
    }

    for (index, example) in few_shots.iter().enumerate() {
        let few = &few_prints[index];
        let main_hit = main.items.iter().find_map(|(line, item)| {
            let fp = item_fingerprint(item);
            match_fingerprint(few, &fp).map(|by| (*line, item, by))
        });
        if let Some((matched_line, item, by)) = main_hit {
            report.main_overlaps.push(DedupeMatch {
                fewshot_index: index + 1,
                fewshot_entry_id: example.entry_id.clone(),
                fewshot_title: example.input.title.clone(),
                matched_line,
                matched_entry_id: item.entry_id.clone(),
                matched_title: item.title.clone(),
                matched_by: by.to_string(),
            });
            continue;
        }
        if let Some(set) = boundary.as_ref() {
            let boundary_hit = set.items.iter().find_map(|(line, item)| {
                let fp = item_fingerprint(item);
                match_fingerprint(few, &fp).map(|by| (*line, item, by))
            });
            if let Some((matched_line, item, by)) = boundary_hit {
                report.boundary_overlaps.push(DedupeMatch {
                    fewshot_index: index + 1,
                    fewshot_entry_id: example.entry_id.clone(),
                    fewshot_title: example.input.title.clone(),
                    matched_line,
                    matched_entry_id: item.entry_id.clone(),
                    matched_title: item.title.clone(),
                    matched_by: by.to_string(),
                });
            }
        }
    }
    if !report.boundary_overlaps.is_empty() {
        report.warnings.push(format!(
            "{} 条 few-shot 与边界集重叠（边界集不参与门线，但会削弱该条的弃权观察效力）",
            report.boundary_overlaps.len()
        ));
    }
    if !report.main_overlaps.is_empty() {
        report.warnings.push(format!(
            "{} 条 few-shot 与主评测集重叠：必须先移除/替换这些示例，否则对应条目成绩被污染（#14 §6.2）",
            report.main_overlaps.len()
        ));
    }
    Ok(report)
}

pub fn render_text(report: &DedupeReport) -> String {
    let mut lines = Vec::new();
    lines.push("== few-shot 池 vs 评测集查重 ==".to_string());
    lines.push(format!(
        "few-shot：{}（{} 条）",
        report.fewshot_file, report.fewshot_count
    ));
    lines.push(format!("主评测集：{}", report.main_set));
    if let Some(boundary) = &report.boundary_set {
        lines.push(format!("边界集：{boundary}"));
    }
    lines.push(format!(
        "主集重叠：{} 条；边界集重叠：{} 条",
        report.main_overlaps.len(),
        report.boundary_overlaps.len()
    ));
    if report.main_overlaps.is_empty() && report.boundary_overlaps.is_empty() {
        lines.push("结果：PASS（主评测集无重叠）".to_string());
    } else {
        for item in report
            .main_overlaps
            .iter()
            .chain(report.boundary_overlaps.iter())
        {
            lines.push(format!(
                "  few-shot #{}（{}）↔ 第 {} 行 {}（{}），依据 {}",
                item.fewshot_index,
                util::truncate_chars(&item.fewshot_title, 40),
                item.matched_line,
                util::truncate_chars(&item.matched_title, 40),
                item.matched_entry_id,
                item.matched_by
            ));
        }
        lines.push(format!(
            "结果：{}",
            if report.has_main_overlaps() {
                "FAIL（主评测集存在重叠）"
            } else {
                "WARN（仅边界集重叠）"
            }
        ));
    }
    if !report.warnings.is_empty() {
        lines.push("警告：".to_string());
        for warning in &report.warnings {
            lines.push(format!("  - {warning}"));
        }
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aether-dedupe-test-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

    fn fewshot_json(entry_id: Option<&str>, title: &str, url: &str) -> String {
        let id = match entry_id {
            Some(id) => format!("\"entry_id\":\"{id}\","),
            None => String::new(),
        };
        format!(
            r#"{{"few_shots":[{{{id}"input":{{"title":"{title}","url":"{url}","media_type":"link","body_excerpt":"b"}},"output":{{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":[]}}}}]}}"#
        )
    }

    fn set_record(entry_id: &str, title: &str, url: &str) -> String {
        format!(
            r#"{{"entry_id":"{entry_id}","title":"{title}","url":"{url}","body_excerpt":"b","media_type":"link","gold":{{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"]}},"difficulty":"easy","split":"visible","eval_set_version":"2026-10-v1","taxonomy_version":"1.1.0"}}"#
        )
    }

    #[test]
    fn detects_entry_id_and_content_overlap() {
        let few = write_tmp(
            "few.json",
            &fewshot_json(Some("01A"), "标题A", "https://example.com/a"),
        );
        let main = write_tmp(
            "main.jsonl",
            &format!(
                "{}\n{}\n",
                set_record("01A", "别的标题", "https://other"),
                set_record("01B", "标题B", "https://example.com/b")
            ),
        );
        let report = check_dedupe(&few, &main, None).unwrap();
        assert!(report.has_main_overlaps());
        assert_eq!(report.main_overlaps[0].matched_by, "entry_id");
    }

    #[test]
    fn detects_title_overlap_even_without_entry_id() {
        let few = write_tmp("few.json", &fewshot_json(None, "标题A", ""));
        let main = write_tmp(
            "main.jsonl",
            &format!("{}\n", set_record("01A", "标题A", "https://example.com/a")),
        );
        let report = check_dedupe(&few, &main, None).unwrap();
        assert_eq!(report.main_overlaps[0].matched_by, "title");
        assert_eq!(report.fewshots_without_entry_id, 1);
    }

    #[test]
    fn boundary_overlap_is_warning_only() {
        let few = write_tmp("few.json", &fewshot_json(None, "标题A", ""));
        let main = write_tmp(
            "main.jsonl",
            &format!("{}\n", set_record("01B", "别的", "https://other")),
        );
        let boundary = write_tmp(
            "boundary.jsonl",
            &format!(
                "{}\n",
                set_record("01A", "标题A", "https://example.com/a")
                    .replace("\"split\":\"visible\"", "\"split\":\"boundary\"")
            ),
        );
        let report = check_dedupe(&few, &main, Some(&boundary)).unwrap();
        assert!(!report.has_main_overlaps());
        assert_eq!(report.boundary_overlaps.len(), 1);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn no_overlap_passes() {
        let few = write_tmp("few.json", &fewshot_json(None, "标题X", ""));
        let main = write_tmp(
            "main.jsonl",
            &format!("{}\n", set_record("01A", "标题A", "https://example.com/a")),
        );
        let report = check_dedupe(&few, &main, None).unwrap();
        assert!(report.main_overlaps.is_empty());
        assert_eq!(report.exit_code(), 0);
    }
}
