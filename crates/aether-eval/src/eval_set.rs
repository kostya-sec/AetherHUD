//! 金标评测集读取、版本校验与金标重映射（#14 §3 / #18 §8.4）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use aether_classifier::taxonomy::{base_version_of, TaxonomyView};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoldLabel {
    pub l1_id: String,
    pub l2_id: String,
    #[serde(default)]
    pub format_tags: Vec<String>,
    #[serde(default)]
    pub feature_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalItem {
    pub entry_id: String,
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub body_excerpt: String,
    #[serde(default = "default_media_type")]
    pub media_type: String,
    #[serde(default)]
    pub gold: Option<GoldLabel>,
    #[serde(default)]
    pub difficulty: Option<String>,
    #[serde(default)]
    pub split: Option<String>,
    #[serde(default)]
    pub eval_set_version: Option<String>,
    #[serde(default)]
    pub taxonomy_version: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
}

fn default_media_type() -> String {
    "link".to_string()
}

#[derive(Debug, thiserror::Error)]
pub enum EvalSetError {
    #[error("读取评测集失败（{path}）：{source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Clone)]
pub struct LoadedEvalSet {
    pub path: PathBuf,
    /// (JSONL 行号, 条目)
    pub items: Vec<(usize, EvalItem)>,
    pub parse_errors: Vec<String>,
    pub eval_set_version: Option<String>,
    pub taxonomy_versions: Vec<String>,
    pub sha256: String,
}

pub fn load_eval_set(path: &Path) -> Result<LoadedEvalSet, EvalSetError> {
    let bytes = std::fs::read(path).map_err(|source| EvalSetError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let sha256 = crate::util::sha256_hex(&bytes);
    let text = String::from_utf8_lossy(&bytes);
    let mut items = Vec::new();
    let mut parse_errors = Vec::new();
    let mut eval_versions: Vec<String> = Vec::new();
    let mut taxonomy_versions: Vec<String> = Vec::new();
    for (index, raw) in text.split('\n').enumerate() {
        let line = index + 1;
        let raw = raw.trim_end_matches('\r');
        if raw.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<EvalItem>(raw) {
            Ok(item) => {
                if let Some(v) = &item.eval_set_version {
                    if !eval_versions.contains(v) {
                        eval_versions.push(v.clone());
                    }
                }
                if let Some(v) = &item.taxonomy_version {
                    if !taxonomy_versions.contains(v) {
                        taxonomy_versions.push(v.clone());
                    }
                }
                items.push((line, item));
            }
            Err(err) => parse_errors.push(format!("第 {line} 行：JSON 解析失败（{err}）")),
        }
    }
    Ok(LoadedEvalSet {
        path: path.to_path_buf(),
        items,
        parse_errors,
        eval_set_version: eval_versions.first().cloned(),
        taxonomy_versions,
        sha256,
    })
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct CompositionStats {
    pub total: usize,
    pub by_l1: BTreeMap<String, usize>,
    pub by_l2: BTreeMap<String, usize>,
    pub difficulty: BTreeMap<String, usize>,
    pub media: BTreeMap<String, usize>,
    pub splits: BTreeMap<String, usize>,
    pub eval_set_versions: BTreeMap<String, usize>,
    pub taxonomy_versions: BTreeMap<String, usize>,
}

impl CompositionStats {
    pub fn ratio(&self, key: &str) -> Option<f64> {
        crate::util::pct(*self.media.get(key).unwrap_or(&0), self.total)
    }
}

pub fn composition(items: &[(usize, EvalItem)]) -> CompositionStats {
    let mut stats = CompositionStats::default();
    for (_, item) in items {
        stats.total += 1;
        if let Some(gold) = &item.gold {
            *stats.by_l1.entry(gold.l1_id.clone()).or_default() += 1;
            *stats.by_l2.entry(gold.l2_id.clone()).or_default() += 1;
        }
        *stats
            .difficulty
            .entry(
                item.difficulty
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string()),
            )
            .or_default() += 1;
        *stats.media.entry(media_class(item)).or_default() += 1;
        if let Some(split) = &item.split {
            *stats.splits.entry(split.clone()).or_default() += 1;
        }
        if let Some(version) = &item.eval_set_version {
            *stats.eval_set_versions.entry(version.clone()).or_default() += 1;
        }
        if let Some(version) = &item.taxonomy_version {
            *stats.taxonomy_versions.entry(version.clone()).or_default() += 1;
        }
    }
    stats
}

pub fn media_class(item: &EvalItem) -> String {
    if item.media_type == "video" {
        "video".to_string()
    } else if item.url.trim().is_empty() {
        "text".to_string()
    } else {
        "web".to_string()
    }
}

/// #14 §3.1 分层覆盖检查（缺口只警告，不中断评测 —— 首版样本分布受收藏限制）。
pub fn composition_warnings(items: &[(usize, EvalItem)], tree: &TaxonomyView) -> Vec<String> {
    let stats = composition(items);
    let mut warnings = Vec::new();
    if stats.total == 0 {
        return vec!["评测集为空".to_string()];
    }
    for l1 in tree.normal_l1_ids() {
        let count = *stats.by_l1.get(l1).unwrap_or(&0);
        if count < 40 {
            warnings.push(format!(
                "L1 {l1} 只有 {count} 条金标（#14 §3.1 要求 >= 40），该组指标置信度有限"
            ));
        }
    }
    for l2 in tree.normal_l2_ids() {
        let count = *stats.by_l2.get(l2).unwrap_or(&0);
        if count < 10 {
            warnings.push(format!(
                "L2 {l2} 只有 {count} 条金标（#14 §3.1 要求 >= 10），该组指标置信度有限"
            ));
        }
    }
    let hard = *stats.difficulty.get("hard").unwrap_or(&0);
    if crate::util::pct(hard, stats.total).unwrap_or(0.0) < 0.20 {
        warnings.push("#14 §3.1：难例占比低于 20%".to_string());
    }
    let video = *stats.media.get("video").unwrap_or(&0);
    if crate::util::pct(video, stats.total).unwrap_or(0.0) < 0.20 {
        warnings.push("#14 §3.1：视频占比低于 20%".to_string());
    }
    let text = *stats.media.get("text").unwrap_or(&0);
    if crate::util::pct(text, stats.total).unwrap_or(0.0) < 0.10 {
        warnings.push("#14 §3.1：纯文本占比低于 10%".to_string());
    }
    let web = *stats.media.get("web").unwrap_or(&0);
    if crate::util::pct(web, stats.total).unwrap_or(0.0) < 0.60 {
        warnings.push("#14 §3.1：网页占比低于 60%".to_string());
    }
    warnings
}

#[derive(Debug, Clone)]
pub struct VersionCheck {
    pub eval_set_version: Option<String>,
    pub eval_set_version_consistent: bool,
    pub eval_set_taxonomy_version: Option<String>,
    pub base_version_matches: Option<bool>,
    pub effective_version_matches: Option<bool>,
    pub comparable: bool,
    pub notes: Vec<String>,
}

pub fn check_versions(
    set: &LoadedEvalSet,
    resolved_version: &str,
    tree: &TaxonomyView,
) -> VersionCheck {
    let mut notes = Vec::new();
    let mut eval_set_version_consistent = true;
    if let Some(v) = &set.eval_set_version {
        if v != resolved_version {
            eval_set_version_consistent = false;
            notes.push(format!(
                "评测集文件内 eval_set_version={v} 与文件名/参数 {resolved_version} 不一致，结果不可比"
            ));
        }
    }
    if set.eval_set_version.is_none() {
        eval_set_version_consistent = false;
        notes.push("评测集未记录 eval_set_version".to_string());
    }
    if set.taxonomy_versions.len() > 1 {
        eval_set_version_consistent = false;
        notes.push(format!(
            "评测集文件内出现多个 taxonomy_version：{}（#14 §3.6 单版本文件才可比）",
            set.taxonomy_versions.join(", ")
        ));
    }
    let recorded = set.taxonomy_versions.first().cloned();
    let (base_matches, effective_matches) = match &recorded {
        None => {
            notes.push("评测集未记录 taxonomy_version，无法校验可比性".to_string());
            (Some(false), Some(false))
        }
        Some(recorded) => {
            let recorded_base = base_version_of(recorded);
            let base_match = recorded_base == tree.base_version;
            let effective_match = recorded == &tree.effective_version;
            if !base_match {
                notes.push(format!(
                    "评测集底表版本 {recorded_base} 与当前底表 {} 不一致：跨 taxonomy 版本不可比（#14 §3.6 / #18 §8.4）",
                    tree.base_version
                ));
            } else if !effective_match {
                notes.push(format!(
                    "评测集标注的生效版本 {recorded} 与当前生效版本 {} 不同：生效分类树已变更，本结果为新基线（#18 §8.4）",
                    tree.effective_version
                ));
            }
            (Some(base_match), Some(effective_match))
        }
    };
    let comparable = eval_set_version_consistent && base_matches == Some(true);
    VersionCheck {
        eval_set_version: set.eval_set_version.clone(),
        eval_set_version_consistent,
        eval_set_taxonomy_version: recorded,
        base_version_matches: base_matches,
        effective_version_matches: effective_matches,
        comparable,
        notes,
    }
}

#[derive(Debug, Clone)]
pub struct PreparedItem {
    pub line: usize,
    pub item: EvalItem,
    pub gold: Option<GoldLabel>,
    pub remapped_from: Option<String>,
    pub invalid_reason: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PrepareResult {
    pub items: Vec<PreparedItem>,
    pub skipped_invalid: usize,
    pub remapped_count: usize,
    pub warnings: Vec<String>,
}

/// 过滤 split、应用 limit、按 merged_into 重映射金标、校验金标引用。
pub fn prepare_items(
    set: &LoadedEvalSet,
    tree: &TaxonomyView,
    boundary: bool,
    split: Option<&str>,
    limit: Option<usize>,
) -> PrepareResult {
    let mut result = PrepareResult::default();
    let mut fetched = 0usize;
    for (line, item) in &set.items {
        if boundary {
            if let Some(s) = &item.split {
                if s != "boundary" {
                    result.warnings.push(format!(
                        "边界集第 {line} 行 split={s}（应为 boundary），仍按边界集处理"
                    ));
                }
            }
            if limit.map(|l| fetched >= l).unwrap_or(false) {
                break;
            }
            fetched += 1;
            result.items.push(PreparedItem {
                line: *line,
                item: item.clone(),
                gold: item.gold.clone(),
                remapped_from: None,
                invalid_reason: None,
            });
            continue;
        }
        let wanted = split.unwrap_or("");
        if item.split.as_deref() != Some(wanted) {
            continue;
        }
        if limit.map(|l| fetched >= l).unwrap_or(false) {
            break;
        }
        fetched += 1;
        let mut prepared = PreparedItem {
            line: *line,
            item: item.clone(),
            gold: item.gold.clone(),
            remapped_from: None,
            invalid_reason: None,
        };
        if let Some(gold) = prepared.gold.take() {
            let mut gold = gold;
            if let Some(target) = tree.remap_l2_id(&gold.l2_id) {
                if gold.l2_id != target {
                    prepared.remapped_from = Some(gold.l2_id.clone());
                    gold.l2_id = target.clone();
                    if let Some(parent) = tree.l2_parent(&target) {
                        gold.l1_id = parent.to_string();
                    }
                    result.remapped_count += 1;
                }
            }
            if !tree.is_normal_l1(&gold.l1_id) {
                prepared.invalid_reason = Some(format!("gold.l1_id 不是正常 L1：{}", gold.l1_id));
            } else if !tree.is_normal_l2(&gold.l2_id) {
                prepared.invalid_reason =
                    Some(format!("gold.l2_id 不是生效树中的正常 L2：{}", gold.l2_id));
            } else if tree.l2_parent(&gold.l2_id) != Some(gold.l1_id.as_str()) {
                prepared.invalid_reason = Some(format!(
                    "gold.l2_id {} 不属于 gold.l1_id {}",
                    gold.l2_id, gold.l1_id
                ));
            } else if gold.format_tags.is_empty()
                || gold.format_tags.len() > 2
                || gold.format_tags.iter().any(|t| !tree.is_format_tag(t))
            {
                prepared.invalid_reason =
                    Some(format!("gold.format_tags 非法：{:?}", gold.format_tags));
            } else if gold.feature_tags.len() > 3 {
                prepared.invalid_reason = Some("gold.feature_tags 超过 3 个".to_string());
            }
            prepared.gold = Some(gold);
        } else {
            prepared.invalid_reason = Some("缺少 gold（主评测集条目必须标注）".to_string());
        }
        if prepared.invalid_reason.is_some() {
            result.skipped_invalid += 1;
        }
        result.items.push(prepared);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_classifier::taxonomy::{base_view_from_json, effective_view_from_json};

    const BASE: &str = r#"{
      "version": "1.1.0",
      "global_l3_tags": {"format_tags": [
        {"id":"tag_tool","name":"工具"},{"id":"tag_tutorial","name":"教程"},
        {"id":"tag_prompt","name":"提示词"},{"id":"tag_workflow","name":"工作流"},
        {"id":"tag_asset","name":"素材/资源"}
      ]},
      "taxonomy": [
        {"id":"l1_inbox","name":"00 未分类","children":[{"id":"l2_manual_review","name":"待人工归档"}]},
        {"id":"l1_tech","name":"03 技术与工程","children":[
          {"id":"l2_coding","name":"编程与低代码","l3_preset_tags":["Cursor"]}
        ]}
      ]
    }"#;

    fn tree() -> TaxonomyView {
        base_view_from_json(BASE).unwrap()
    }

    fn write_set(content: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "aether-evalset-{}-{}.jsonl",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::write(&path, content).unwrap();
        path
    }

    fn item(l1: &str, l2: &str, split: &str) -> String {
        format!(
            r#"{{"entry_id":"01A","title":"t","url":"https://example.com","body_excerpt":"b","media_type":"link","gold":{{"l1_id":"{l1}","l2_id":"{l2}","format_tags":["tag_tool"]}},"difficulty":"easy","split":"{split}","eval_set_version":"2026-10-v1","taxonomy_version":"1.1.0"}}"#
        )
    }

    #[test]
    fn loads_and_filters_split() {
        let content = format!(
            "{}\n{}\nnot json\n",
            item("l1_tech", "l2_coding", "visible"),
            item("l1_tech", "l2_coding", "blind")
        );
        let set = load_eval_set(&write_set(&content)).unwrap();
        assert_eq!(set.items.len(), 2);
        assert_eq!(set.parse_errors.len(), 1);
        assert_eq!(set.eval_set_version.as_deref(), Some("2026-10-v1"));
        let prepared = prepare_items(&set, &tree(), false, Some("visible"), None);
        assert_eq!(prepared.items.len(), 1);
        assert_eq!(prepared.items[0].gold.as_ref().unwrap().l2_id, "l2_coding");
    }

    #[test]
    fn remaps_merged_self_built_l2() {
        let base = tree();
        let eff = r#"{
          "version": "1.1.0+u2",
          "base_version": "1.1.0",
          "merged_into": [{"from":"l2u_old","to":"l2_coding"}],
          "global_l3_tags": {"format_tags": [{"id":"tag_tool","name":"工具"}]},
          "taxonomy": [
            {"id":"l1_inbox","name":"00 未分类","children":[{"id":"l2_manual_review","name":"待人工归档"}]},
            {"id":"l1_tech","name":"03 技术与工程","children":[{"id":"l2_coding","name":"编程与低代码"}]}
          ]
        }"#;
        let eff_tree = effective_view_from_json(eff, None).unwrap();
        let _ = base;
        let content = format!("{}\n", item("l1_tech", "l2u_old", "visible"));
        let set = load_eval_set(&write_set(&content)).unwrap();
        let prepared = prepare_items(&set, &eff_tree, false, Some("visible"), None);
        assert_eq!(prepared.remapped_count, 1);
        assert_eq!(prepared.items[0].remapped_from.as_deref(), Some("l2u_old"));
        assert_eq!(prepared.items[0].gold.as_ref().unwrap().l2_id, "l2_coding");
        assert!(prepared.items[0].invalid_reason.is_none());
    }

    #[test]
    fn invalid_gold_is_skipped_not_fatal() {
        let content = format!("{}\n", item("l1_tech", "l2_missing", "visible"));
        let set = load_eval_set(&write_set(&content)).unwrap();
        let prepared = prepare_items(&set, &tree(), false, Some("visible"), None);
        assert_eq!(prepared.skipped_invalid, 1);
        assert!(prepared.items[0].invalid_reason.is_some());
    }

    #[test]
    fn version_check_flags_base_mismatch() {
        let tree = tree();
        let content = format!(
            "{}\n",
            item("l1_tech", "l2_coding", "visible").replace("1.1.0", "0.9.0")
        );
        let set = load_eval_set(&write_set(&content)).unwrap();
        let check = check_versions(&set, "2026-10-v1", &tree);
        assert_eq!(check.base_version_matches, Some(false));
        assert!(!check.comparable);
        assert!(check.notes.iter().any(|n| n.contains("不可比")));
    }

    #[test]
    fn version_check_effective_change_is_new_baseline_note() {
        let tree = tree();
        let content = format!(
            "{}\n",
            item("l1_tech", "l2_coding", "visible").replace("1.1.0", "1.1.0+u1")
        );
        let set = load_eval_set(&write_set(&content)).unwrap();
        let check = check_versions(&set, "2026-10-v1", &tree);
        assert_eq!(check.base_version_matches, Some(true));
        assert_eq!(check.effective_version_matches, Some(false));
        assert!(check.comparable);
        assert!(check.notes.iter().any(|n| n.contains("新基线")));
    }
}
