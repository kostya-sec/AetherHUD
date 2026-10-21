//! 指标计算（#14 §2.3 口径）。

use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use aether_classifier::engine::ClassificationOutcome;
use aether_classifier::provider::Usage;
use aether_classifier::taxonomy::{TaxonomyView, L1_INBOX, L2_MANUAL_REVIEW};
use aether_classifier::OutcomeStatus;

use crate::eval_set::{EvalItem, GoldLabel};
use crate::util::{fmt_pct, pct};

#[derive(Debug, Clone)]
pub struct RunItem {
    pub line: usize,
    pub item: EvalItem,
    pub gold: Option<GoldLabel>,
    pub remapped_from: Option<String>,
    pub outcome: ClassificationOutcome,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct GroupStats {
    pub total: usize,
    pub classified: usize,
    pub failed: usize,
    pub abstained: usize,
    pub l1_correct: usize,
    pub l2_correct: usize,
    pub l2_given_l1_correct: usize,
}

impl GroupStats {
    pub fn l1_accuracy(&self) -> Option<f64> {
        pct(self.l1_correct, self.classified)
    }

    pub fn l2_accuracy(&self) -> Option<f64> {
        pct(self.l2_correct, self.classified)
    }

    pub fn abstain_rate(&self) -> Option<f64> {
        pct(self.abstained, self.classified)
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct ErrorItem {
    pub line: usize,
    pub entry_id: String,
    pub title: String,
    pub difficulty: String,
    pub gold_l1: String,
    pub gold_l2: String,
    pub pred_l1: String,
    pub pred_l2: String,
    pub kind: String,
    pub gold_format_tags: Vec<String>,
    pub pred_format_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FailureItem {
    pub line: usize,
    pub entry_id: String,
    pub title: String,
    pub difficulty: String,
    pub status: String,
    pub reason: String,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct EvalMetrics {
    pub total_evaluated: usize,
    pub classified: usize,
    pub failed: usize,
    pub format_failed: usize,
    pub unavailable: usize,
    pub abstained: usize,
    pub l1_correct: usize,
    pub l2_correct: usize,
    pub l2_given_l1_correct: usize,
    pub format_tp: usize,
    pub format_fp: usize,
    pub format_fn: usize,
    pub by_l1: BTreeMap<String, GroupStats>,
    pub by_l2: BTreeMap<String, GroupStats>,
    pub by_difficulty: BTreeMap<String, GroupStats>,
    pub confusion: BTreeMap<String, BTreeMap<String, usize>>,
    pub errors: Vec<ErrorItem>,
    pub failures: Vec<FailureItem>,
    pub usage: Usage,
    pub latency_ms_total: u64,
    pub api_calls: usize,
    pub http_retries: usize,
    pub repaired_items: usize,
    /// 修复重试触发原因（ValidationCode -> 次数），供 prompt 迭代分析。
    pub repair_issues: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct MetricRates {
    pub l1_accuracy: Option<f64>,
    pub l2_accuracy: Option<f64>,
    pub l2_given_l1: Option<f64>,
    pub format_f1: Option<f64>,
    pub abstain_rate: Option<f64>,
    pub availability: Option<f64>,
}

impl EvalMetrics {
    pub fn l1_accuracy(&self) -> Option<f64> {
        pct(self.l1_correct, self.classified)
    }

    pub fn l2_accuracy(&self) -> Option<f64> {
        pct(self.l2_correct, self.classified)
    }

    pub fn l2_given_l1(&self) -> Option<f64> {
        pct(self.l2_given_l1_correct, self.l1_correct)
    }

    pub fn format_f1(&self) -> Option<f64> {
        let denom = 2 * self.format_tp + self.format_fp + self.format_fn;
        if denom == 0 {
            None
        } else {
            Some(2.0 * self.format_tp as f64 / denom as f64)
        }
    }

    pub fn abstain_rate(&self) -> Option<f64> {
        pct(self.abstained, self.classified)
    }

    pub fn availability(&self) -> Option<f64> {
        pct(self.total_evaluated - self.failed, self.total_evaluated)
    }

    pub fn rates(&self) -> MetricRates {
        MetricRates {
            l1_accuracy: self.l1_accuracy(),
            l2_accuracy: self.l2_accuracy(),
            l2_given_l1: self.l2_given_l1(),
            format_f1: self.format_f1(),
            abstain_rate: self.abstain_rate(),
            availability: self.availability(),
        }
    }
}

fn difficulty_of(item: &EvalItem) -> String {
    item.difficulty
        .clone()
        .unwrap_or_else(|| "unknown".to_string())
}

fn group_mut<'a>(map: &'a mut BTreeMap<String, GroupStats>, key: &str) -> &'a mut GroupStats {
    map.entry(key.to_string()).or_default()
}

pub fn compute(items: &[RunItem], _tree: &TaxonomyView) -> EvalMetrics {
    let mut m = EvalMetrics::default();
    for run in items {
        let Some(gold) = &run.gold else {
            continue;
        };
        m.total_evaluated += 1;
        let difficulty = difficulty_of(&run.item);
        m.usage.add(&run.outcome.usage);
        m.latency_ms_total += run.outcome.elapsed_ms;
        m.api_calls += run.outcome.total_calls();
        m.http_retries += run.outcome.http_retry_count();
        if run.outcome.repair_used {
            m.repaired_items += 1;
            for issue in &run.outcome.repair_trigger_issues {
                *m.repair_issues
                    .entry(format!("{:?}", issue.code))
                    .or_default() += 1;
            }
        }
        let l1_group = group_mut(&mut m.by_l1, &gold.l1_id);
        let l2_group = group_mut(&mut m.by_l2, &gold.l2_id);
        let diff_group = group_mut(&mut m.by_difficulty, &difficulty);
        l1_group.total += 1;
        l2_group.total += 1;
        diff_group.total += 1;

        match run.outcome.status {
            OutcomeStatus::FormatInvalid => {
                m.failed += 1;
                m.format_failed += 1;
                l1_group.failed += 1;
                l2_group.failed += 1;
                diff_group.failed += 1;
                m.failures.push(FailureItem {
                    line: run.line,
                    entry_id: run.item.entry_id.clone(),
                    title: run.item.title.clone(),
                    difficulty,
                    status: "format_invalid".to_string(),
                    reason: run
                        .outcome
                        .failure_reason
                        .clone()
                        .unwrap_or_else(|| "format_invalid".to_string()),
                    issues: run
                        .outcome
                        .issues
                        .iter()
                        .map(|i| format!("{:?}: {}", i.code, i.detail))
                        .collect(),
                });
            }
            OutcomeStatus::Unavailable => {
                m.failed += 1;
                m.unavailable += 1;
                l1_group.failed += 1;
                l2_group.failed += 1;
                diff_group.failed += 1;
                m.failures.push(FailureItem {
                    line: run.line,
                    entry_id: run.item.entry_id.clone(),
                    title: run.item.title.clone(),
                    difficulty,
                    status: "unavailable".to_string(),
                    reason: run
                        .outcome
                        .failure_reason
                        .clone()
                        .unwrap_or_else(|| "unavailable".to_string()),
                    issues: Vec::new(),
                });
            }
            OutcomeStatus::Ok => {
                let Some(classification) = &run.outcome.classification else {
                    continue;
                };
                m.classified += 1;
                l1_group.classified += 1;
                l2_group.classified += 1;
                diff_group.classified += 1;
                let abstained = run.outcome.abstained
                    || (classification.l1_id == L1_INBOX
                        && classification.l2_id == L2_MANUAL_REVIEW);
                if abstained {
                    m.abstained += 1;
                    l1_group.abstained += 1;
                    l2_group.abstained += 1;
                    diff_group.abstained += 1;
                }
                let l1_ok = classification.l1_id == gold.l1_id;
                let l2_ok = classification.l2_id == gold.l2_id;
                if l1_ok {
                    m.l1_correct += 1;
                    l1_group.l1_correct += 1;
                    l2_group.l1_correct += 1;
                    diff_group.l1_correct += 1;
                }
                if l2_ok {
                    m.l2_correct += 1;
                    l1_group.l2_correct += 1;
                    l2_group.l2_correct += 1;
                    diff_group.l2_correct += 1;
                }
                if l1_ok && l2_ok {
                    m.l2_given_l1_correct += 1;
                    l1_group.l2_given_l1_correct += 1;
                    l2_group.l2_given_l1_correct += 1;
                    diff_group.l2_given_l1_correct += 1;
                }
                update_format_counts(&mut m, &gold.format_tags, &classification.format_tags);
                *m.confusion
                    .entry(gold.l1_id.clone())
                    .or_default()
                    .entry(classification.l1_id.clone())
                    .or_default() += 1;
                if !l1_ok || !l2_ok {
                    let kind = if abstained {
                        "abstain"
                    } else if !l1_ok {
                        "l1_wrong"
                    } else {
                        "l2_wrong"
                    };
                    m.errors.push(ErrorItem {
                        line: run.line,
                        entry_id: run.item.entry_id.clone(),
                        title: run.item.title.clone(),
                        difficulty,
                        gold_l1: gold.l1_id.clone(),
                        gold_l2: gold.l2_id.clone(),
                        pred_l1: classification.l1_id.clone(),
                        pred_l2: classification.l2_id.clone(),
                        kind: kind.to_string(),
                        gold_format_tags: gold.format_tags.clone(),
                        pred_format_tags: classification.format_tags.clone(),
                    });
                }
            }
        }
    }
    m.errors.sort_by_key(|e| {
        let difficulty_rank = if e.difficulty == "hard" { 0 } else { 1 };
        (difficulty_rank, e.line)
    });
    m
}

fn update_format_counts(m: &mut EvalMetrics, gold: &[String], pred: &[String]) {
    let gold_set: HashSet<&String> = gold.iter().collect();
    let pred_set: HashSet<&String> = pred.iter().collect();
    for tag in pred {
        if gold_set.contains(tag) {
            m.format_tp += 1;
        } else {
            m.format_fp += 1;
        }
    }
    for tag in gold {
        if !pred_set.contains(tag) {
            m.format_fn += 1;
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GateCheck {
    pub key: String,
    pub label: String,
    pub target: f64,
    pub floor: f64,
    pub higher_is_better: bool,
    pub actual: Option<f64>,
    pub floor_pass: Option<bool>,
    pub target_pass: Option<bool>,
}

pub fn gate_checks(rates: &MetricRates) -> Vec<GateCheck> {
    let mut out = Vec::new();
    let mut push = |key: &str,
                    label: &str,
                    target: f64,
                    floor: f64,
                    higher_is_better: bool,
                    actual: Option<f64>| {
        let floor_pass = actual.map(|v| {
            if higher_is_better {
                v >= floor - 1e-9
            } else {
                v <= floor + 1e-9
            }
        });
        let target_pass = actual.map(|v| {
            if higher_is_better {
                v >= target - 1e-9
            } else {
                v <= target + 1e-9
            }
        });
        out.push(GateCheck {
            key: key.to_string(),
            label: label.to_string(),
            target,
            floor,
            higher_is_better,
            actual,
            floor_pass,
            target_pass,
        });
    };
    push(
        "l1_accuracy",
        "L1 准确率",
        0.95,
        0.90,
        true,
        rates.l1_accuracy,
    );
    push(
        "l2_accuracy",
        "L2 准确率",
        0.90,
        0.85,
        true,
        rates.l2_accuracy,
    );
    push(
        "format_f1",
        "形态标签 micro-F1",
        0.80,
        0.75,
        true,
        rates.format_f1,
    );
    push(
        "abstain_rate",
        "弃权率",
        0.05,
        0.08,
        false,
        rates.abstain_rate,
    );
    push(
        "availability",
        "分类可用性",
        0.99,
        0.99,
        true,
        rates.availability,
    );
    out
}

pub fn gate_overall(checks: &[GateCheck]) -> Option<bool> {
    let mut saw = false;
    for check in checks {
        if let Some(pass) = check.floor_pass {
            saw = true;
            if !pass {
                return Some(false);
            }
        }
    }
    if saw {
        Some(true)
    } else {
        None
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BoundaryRow {
    pub line: usize,
    pub entry_id: String,
    pub title: String,
    pub difficulty: String,
    pub status: String,
    pub abstained: bool,
    pub pred_l1: String,
    pub pred_l2: String,
    pub suggestion: Option<serde_json::Value>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct BoundaryMetrics {
    pub total: usize,
    pub classified: usize,
    pub failed: usize,
    pub format_failed: usize,
    pub unavailable: usize,
    pub abstained: usize,
    pub by_pred_l1: BTreeMap<String, usize>,
    pub by_pred_l2: BTreeMap<String, usize>,
    pub with_suggestion: usize,
    pub with_nearest_l2: usize,
    pub with_new_l2: usize,
    pub new_l2_by_parent: BTreeMap<String, usize>,
    pub rows: Vec<BoundaryRow>,
    pub usage: Usage,
    pub latency_ms_total: u64,
}

pub fn compute_boundary(items: &[RunItem]) -> BoundaryMetrics {
    let mut m = BoundaryMetrics::default();
    for run in items {
        m.total += 1;
        m.usage.add(&run.outcome.usage);
        m.latency_ms_total += run.outcome.elapsed_ms;
        let difficulty = difficulty_of(&run.item);
        let status = match run.outcome.status {
            OutcomeStatus::Ok => "ok",
            OutcomeStatus::FormatInvalid => "format_invalid",
            OutcomeStatus::Unavailable => "unavailable",
        };
        let mut pred_l1 = String::new();
        let mut pred_l2 = String::new();
        let mut abstained = false;
        let mut suggestion = None;
        if run.outcome.status == OutcomeStatus::Ok {
            m.classified += 1;
            if let Some(classification) = &run.outcome.classification {
                pred_l1 = classification.l1_id.clone();
                pred_l2 = classification.l2_id.clone();
                abstained = run.outcome.abstained
                    || (classification.l1_id == L1_INBOX
                        && classification.l2_id == L2_MANUAL_REVIEW);
                if abstained {
                    m.abstained += 1;
                }
                if let Some(s) = &classification.suggestion {
                    m.with_suggestion += 1;
                    sug_stats(&mut m, s);
                    suggestion = serde_json::to_value(s).ok();
                }
                *m.by_pred_l1.entry(pred_l1.clone()).or_default() += 1;
                *m.by_pred_l2.entry(pred_l2.clone()).or_default() += 1;
            }
        } else {
            m.failed += 1;
            if run.outcome.status == OutcomeStatus::FormatInvalid {
                m.format_failed += 1;
            } else {
                m.unavailable += 1;
            }
        }
        m.rows.push(BoundaryRow {
            line: run.line,
            entry_id: run.item.entry_id.clone(),
            title: run.item.title.clone(),
            difficulty,
            status: status.to_string(),
            abstained,
            pred_l1,
            pred_l2,
            suggestion,
            failure_reason: run.outcome.failure_reason.clone(),
        });
    }
    m
}

fn sug_stats(m: &mut BoundaryMetrics, suggestion: &aether_classifier::Suggestion) {
    if suggestion.nearest_l2_id.is_some() {
        m.with_nearest_l2 += 1;
    }
    if let Some(new_l2) = &suggestion.new_l2 {
        m.with_new_l2 += 1;
        *m.new_l2_by_parent
            .entry(new_l2.parent_l1_id.clone())
            .or_default() += 1;
    }
}

pub fn gate_summary_line(checks: &[GateCheck]) -> String {
    let overall = gate_overall(checks);
    match overall {
        Some(true) => "PASS".to_string(),
        Some(false) => "FAIL".to_string(),
        None => "—".to_string(),
    }
}

pub fn rates_table(rates: &MetricRates) -> Vec<(&'static str, String)> {
    vec![
        ("L1 准确率", fmt_pct(rates.l1_accuracy)),
        ("L2 准确率", fmt_pct(rates.l2_accuracy)),
        ("L2|L1 条件准确率", fmt_pct(rates.l2_given_l1)),
        ("形态标签 micro-F1", crate::util::fmt_rate(rates.format_f1)),
        ("弃权率", fmt_pct(rates.abstain_rate)),
        ("分类可用性", fmt_pct(rates.availability)),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_classifier::engine::{CallPurpose, CallRecord, ClassificationOutcome};
    use aether_classifier::provider::Usage;
    use aether_classifier::schema::Classification;
    use aether_classifier::taxonomy::load_base_taxonomy;

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
          {"id":"l2_coding","name":"编程与低代码"},
          {"id":"l2_prompt_agent","name":"提示工程与智能体"}
        ]},
        {"id":"l1_biz","name":"04 商业与认知","children":[
          {"id":"l2_insight","name":"行业观察与研报"}
        ]}
      ]
    }"#;

    fn tree() -> TaxonomyView {
        let path = std::env::temp_dir().join(format!(
            "aether-metrics-test-{}-{}.json",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::write(&path, BASE).unwrap();
        load_base_taxonomy(&path).unwrap()
    }

    fn item(line: usize, l1: &str, l2: &str, difficulty: &str) -> RunItem {
        RunItem {
            line,
            item: EvalItem {
                entry_id: format!("01ITEM{line:02}"),
                title: format!("title {line}"),
                url: String::new(),
                body_excerpt: String::new(),
                media_type: "link".to_string(),
                gold: None,
                difficulty: Some(difficulty.to_string()),
                split: Some("visible".to_string()),
                eval_set_version: Some("2026-10-v1".to_string()),
                taxonomy_version: Some("1.1.0".to_string()),
                notes: None,
            },
            gold: Some(GoldLabel {
                l1_id: l1.to_string(),
                l2_id: l2.to_string(),
                format_tags: vec!["tag_tool".to_string()],
                feature_tags: vec![],
            }),
            remapped_from: None,
            outcome: ClassificationOutcome {
                status: OutcomeStatus::Ok,
                classification: None,
                abstained: false,
                raw_output: None,
                first_raw_output: None,
                repair_used: false,
                issues: Vec::new(),
                repair_trigger_issues: Vec::new(),
                failure_reason: None,
                calls: Vec::new(),
                usage: Usage::default(),
                elapsed_ms: 10,
            },
        }
    }

    fn classify(mut run: RunItem, l1: &str, l2: &str, tags: &[&str]) -> RunItem {
        let classification = Classification {
            l1_id: l1.to_string(),
            l2_id: l2.to_string(),
            format_tags: tags.iter().map(|t| t.to_string()).collect(),
            feature_tags: vec![],
            suggestion: None,
        };
        run.outcome.classification = Some(classification);
        run.outcome.abstained = l1 == L1_INBOX && l2 == L2_MANUAL_REVIEW;
        run.outcome.calls = vec![CallRecord {
            purpose: CallPurpose::Initial,
            attempt: 1,
            ok: true,
            http_status: None,
            error: None,
            latency_ms: 10,
            usage: Usage {
                prompt_tokens: 100,
                completion_tokens: 10,
            },
        }];
        run.outcome.usage = Usage {
            prompt_tokens: 100,
            completion_tokens: 10,
        };
        run
    }

    fn fail(mut run: RunItem, status: OutcomeStatus) -> RunItem {
        run.outcome.status = status;
        run.outcome.classification = None;
        run
    }

    #[test]
    fn failures_excluded_from_accuracy_denominator() {
        let runs = vec![
            classify(
                item(1, "l1_tech", "l2_coding", "easy"),
                "l1_tech",
                "l2_coding",
                &["tag_tool"],
            ),
            classify(
                item(2, "l1_tech", "l2_coding", "easy"),
                "l1_biz",
                "l2_insight",
                &["tag_asset"],
            ),
            fail(
                item(3, "l1_biz", "l2_insight", "hard"),
                OutcomeStatus::FormatInvalid,
            ),
            fail(
                item(4, "l1_tech", "l2_prompt_agent", "hard"),
                OutcomeStatus::Unavailable,
            ),
        ];
        let m = compute(&runs, &tree());
        assert_eq!(m.total_evaluated, 4);
        assert_eq!(m.classified, 2);
        assert_eq!(m.failed, 2);
        assert_eq!(m.l1_accuracy(), Some(0.5));
        assert_eq!(m.l2_accuracy(), Some(0.5));
        assert_eq!(m.availability(), Some(0.5));
    }

    #[test]
    fn abstention_counts_as_error_but_not_failure() {
        let runs = vec![
            classify(
                item(1, "l1_tech", "l2_coding", "easy"),
                L1_INBOX,
                L2_MANUAL_REVIEW,
                &["tag_tool"],
            ),
            classify(
                item(2, "l1_tech", "l2_coding", "easy"),
                "l1_tech",
                "l2_coding",
                &["tag_tool"],
            ),
        ];
        let m = compute(&runs, &tree());
        assert_eq!(m.classified, 2);
        assert_eq!(m.abstained, 1);
        assert_eq!(m.abstain_rate(), Some(0.5));
        assert_eq!(m.l1_accuracy(), Some(0.5));
        assert_eq!(m.errors.len(), 1);
        assert_eq!(m.errors[0].kind, "abstain");
    }

    #[test]
    fn format_micro_f1_counts_tp_fp_fn() {
        let runs = vec![
            // gold tool, pred tool => TP
            classify(
                item(1, "l1_tech", "l2_coding", "easy"),
                "l1_tech",
                "l2_coding",
                &["tag_tool"],
            ),
            // gold tool, pred asset => FP + FN
            classify(
                item(2, "l1_tech", "l2_coding", "easy"),
                "l1_tech",
                "l2_coding",
                &["tag_asset"],
            ),
            // gold tool, pred tool+tutorial => TP + FP
            classify(
                item(3, "l1_tech", "l2_coding", "easy"),
                "l1_tech",
                "l2_coding",
                &["tag_tool", "tag_tutorial"],
            ),
        ];
        let m = compute(&runs, &tree());
        assert_eq!(m.format_tp, 2);
        assert_eq!(m.format_fp, 2);
        assert_eq!(m.format_fn, 1);
        let f1 = m.format_f1().unwrap();
        assert!((f1 - 4.0 / 7.0).abs() < 1e-9);
    }

    #[test]
    fn groups_and_confusion_matrix() {
        let runs = vec![
            classify(
                item(1, "l1_tech", "l2_coding", "hard"),
                "l1_tech",
                "l2_coding",
                &["tag_tool"],
            ),
            classify(
                item(2, "l1_tech", "l2_prompt_agent", "easy"),
                "l1_biz",
                "l2_insight",
                &["tag_asset"],
            ),
        ];
        let m = compute(&runs, &tree());
        assert_eq!(m.by_l1["l1_tech"].l1_correct, 1);
        assert_eq!(m.by_l1["l1_tech"].l2_accuracy(), Some(0.5));
        assert_eq!(m.by_difficulty["hard"].l1_accuracy(), Some(1.0));
        assert_eq!(m.by_difficulty["easy"].l1_accuracy(), Some(0.0));
        assert_eq!(m.confusion["l1_tech"]["l1_tech"], 1);
        assert_eq!(m.confusion["l1_tech"]["l1_biz"], 1);
    }

    #[test]
    fn gate_checks_floor_and_target() {
        let rates = MetricRates {
            l1_accuracy: Some(0.92),
            l2_accuracy: Some(0.86),
            l2_given_l1: Some(0.93),
            format_f1: Some(0.70),
            abstain_rate: Some(0.03),
            availability: Some(1.0),
        };
        let checks = gate_checks(&rates);
        let by_key: BTreeMap<_, _> = checks.iter().map(|c| (c.key.as_str(), c)).collect();
        assert_eq!(by_key["l1_accuracy"].floor_pass, Some(true));
        assert_eq!(by_key["l1_accuracy"].target_pass, Some(false));
        assert_eq!(by_key["format_f1"].floor_pass, Some(false));
        assert_eq!(by_key["abstain_rate"].floor_pass, Some(true));
        assert_eq!(gate_overall(&checks), Some(false));
    }

    #[test]
    fn boundary_metrics_collect_suggestions() {
        let mut run = classify(
            item(1, "l1_tech", "l2_coding", "hard"),
            L1_INBOX,
            L2_MANUAL_REVIEW,
            &["tag_tool"],
        );
        run.gold = None;
        if let Some(c) = run.outcome.classification.as_mut() {
            c.suggestion = Some(aether_classifier::Suggestion {
                nearest_l2_id: Some("l2_coding".to_string()),
                new_l2: Some(aether_classifier::NewL2 {
                    parent_l1_id: "l1_tech".to_string(),
                    name: "交易与投资".to_string(),
                }),
            });
        }
        let m = compute_boundary(&[run]);
        assert_eq!(m.total, 1);
        assert_eq!(m.abstained, 1);
        assert_eq!(m.with_suggestion, 1);
        assert_eq!(m.with_nearest_l2, 1);
        assert_eq!(m.with_new_l2, 1);
        assert_eq!(m.new_l2_by_parent["l1_tech"], 1);
    }
}
