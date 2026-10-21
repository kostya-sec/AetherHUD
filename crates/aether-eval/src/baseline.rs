//! 同条件基线：索引、对比与不可覆盖的基线报告（#14 §4.4 / §2.5）。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::metrics::MetricRates;
use crate::util;

pub const BASELINE_INDEX_FILE: &str = "baseline-index.json";
const INDEX_SCHEMA: &str = "aether-eval-baseline/v1";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BaselineIndex {
    pub schema: String,
    pub entries: BTreeMap<String, BaselineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineEntry {
    pub combo_key: String,
    pub provider: String,
    pub model: String,
    pub prompt_version: u32,
    pub eval_set_version: String,
    pub taxonomy_base_version: String,
    pub taxonomy_effective_version: String,
    pub split: String,
    pub report_path: String,
    pub created_at: String,
    pub eval_set_sha256: String,
    pub rates: StoredRates,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StoredRates {
    pub l1_accuracy: Option<f64>,
    pub l2_accuracy: Option<f64>,
    pub l2_given_l1: Option<f64>,
    pub format_f1: Option<f64>,
    pub abstain_rate: Option<f64>,
    pub availability: Option<f64>,
}

impl From<&MetricRates> for StoredRates {
    fn from(rates: &MetricRates) -> Self {
        Self {
            l1_accuracy: rates.l1_accuracy,
            l2_accuracy: rates.l2_accuracy,
            l2_given_l1: rates.l2_given_l1,
            format_f1: rates.format_f1,
            abstain_rate: rates.abstain_rate,
            availability: rates.availability,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComboInfo {
    pub provider: String,
    pub model: String,
    pub prompt_version: u32,
    pub eval_set_version: String,
    pub taxonomy_base_version: String,
    pub taxonomy_effective_version: String,
    pub split: String,
}

pub fn combo_key(info: &ComboInfo) -> String {
    format!(
        "{}|{}|v{}|{}|{}|{}|{}",
        info.provider,
        info.model,
        info.prompt_version,
        info.eval_set_version,
        info.taxonomy_base_version,
        info.taxonomy_effective_version,
        info.split
    )
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricDelta {
    pub key: String,
    pub label: String,
    pub baseline: Option<f64>,
    pub current: Option<f64>,
    pub delta: Option<f64>,
    pub as_percent: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BaselineComparison {
    pub combo_key: String,
    pub is_baseline: bool,
    pub baseline_report: Option<String>,
    pub baseline_created_at: Option<String>,
    pub eval_set_same: bool,
    pub comparable_versions: bool,
    pub deltas: Vec<MetricDelta>,
    pub warnings: Vec<String>,
}

impl BaselineComparison {
    pub fn new_baseline(combo_key: &str, comparable_versions: bool) -> Self {
        Self {
            combo_key: combo_key.to_string(),
            is_baseline: true,
            baseline_report: None,
            baseline_created_at: None,
            eval_set_same: true,
            comparable_versions,
            deltas: Vec::new(),
            warnings: Vec::new(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BaselineError {
    #[error("读取基线索引失败（{path}）：{source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("解析基线索引失败（{path}）：{source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("写入基线索引失败（{path}）：{source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

pub fn load_index(path: &Path) -> Result<BaselineIndex, BaselineError> {
    if !path.is_file() {
        return Ok(BaselineIndex {
            schema: INDEX_SCHEMA.to_string(),
            entries: BTreeMap::new(),
        });
    }
    let text = std::fs::read_to_string(path).map_err(|source| BaselineError::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut index: BaselineIndex =
        serde_json::from_str(&text).map_err(|source| BaselineError::Json {
            path: path.display().to_string(),
            source,
        })?;
    if index.schema.is_empty() {
        index.schema = INDEX_SCHEMA.to_string();
    }
    Ok(index)
}

pub fn save_index(path: &Path, index: &BaselineIndex) -> Result<(), BaselineError> {
    let text = serde_json::to_string_pretty(index).unwrap_or_else(|_| "{}".to_string());
    util::write_atomic(path, &text).map_err(|source| BaselineError::Write {
        path: path.display().to_string(),
        source,
    })
}

fn metric_specs() -> Vec<(&'static str, &'static str, bool)> {
    vec![
        ("l1_accuracy", "L1 准确率", true),
        ("l2_accuracy", "L2 准确率", true),
        ("l2_given_l1", "L2|L1 条件准确率", true),
        ("format_f1", "形态标签 micro-F1", false),
        ("abstain_rate", "弃权率", true),
        ("availability", "分类可用性", true),
    ]
}

fn value_of(rates: &StoredRates, key: &str) -> Option<f64> {
    match key {
        "l1_accuracy" => rates.l1_accuracy,
        "l2_accuracy" => rates.l2_accuracy,
        "l2_given_l1" => rates.l2_given_l1,
        "format_f1" => rates.format_f1,
        "abstain_rate" => rates.abstain_rate,
        "availability" => rates.availability,
        _ => None,
    }
}

/// 读取或建立基线：同一组合第一次运行为基线；之后只与基线比较（绝不覆盖基线）。
pub fn compare_and_register(
    index_path: &Path,
    combo: &ComboInfo,
    eval_set_sha256: &str,
    report_path: &Path,
    rates: &MetricRates,
    created_at: &str,
    comparable_versions: bool,
) -> Result<BaselineComparison, BaselineError> {
    let key = combo_key(combo);
    let mut index = load_index(index_path)?;
    let existing = index.entries.get(&key).cloned();
    if let Some(entry) = existing {
        let baseline_report = PathBuf::from(&entry.report_path);
        let mut warnings = Vec::new();
        let eval_set_same = entry.eval_set_sha256 == eval_set_sha256;
        if !comparable_versions {
            warnings.push(
                "版本校验未通过（底表或评测集版本不一致），不与基线比较差值（#14 §3.6）"
                    .to_string(),
            );
        }
        if !eval_set_same {
            warnings.push(format!(
                "评测集内容与基线不同（同一版本号但文件哈希变化）：{}；本次重设为基线",
                entry.report_path
            ));
        }
        if !baseline_report.is_file() {
            warnings.push(format!(
                "基线报告文件缺失：{}；本次重设为基线",
                entry.report_path
            ));
        }
        if !comparable_versions || !eval_set_same || !baseline_report.is_file() {
            register_baseline(
                &mut index,
                &key,
                combo,
                eval_set_sha256,
                report_path,
                rates,
                created_at,
            );
            save_index(index_path, &index)?;
            return Ok(BaselineComparison {
                combo_key: key,
                is_baseline: true,
                baseline_report: Some(entry.report_path),
                baseline_created_at: Some(entry.created_at),
                eval_set_same,
                comparable_versions,
                deltas: Vec::new(),
                warnings,
            });
        }
        let current = StoredRates::from(rates);
        let mut deltas = Vec::new();
        for (metric_key, label, as_percent) in metric_specs() {
            let baseline = value_of(&entry.rates, metric_key);
            let current_value = value_of(&current, metric_key);
            let delta = match (baseline, current_value) {
                (Some(b), Some(c)) => Some(c - b),
                _ => None,
            };
            deltas.push(MetricDelta {
                key: metric_key.to_string(),
                label: label.to_string(),
                baseline,
                current: current_value,
                delta,
                as_percent,
            });
        }
        for delta in &deltas {
            let Some(value) = delta.delta else { continue };
            let regression = if delta.key == "abstain_rate" {
                value > 0.02
            } else {
                value < -0.02
            };
            if regression {
                warnings.push(format!(
                    "回归超过 2pt：{} {} → {}（#14 §2.5：应回滚本次改动）",
                    delta.label,
                    crate::util::fmt_pct(delta.baseline),
                    crate::util::fmt_pct(delta.current)
                ));
            }
        }
        Ok(BaselineComparison {
            combo_key: key,
            is_baseline: false,
            baseline_report: Some(entry.report_path),
            baseline_created_at: Some(entry.created_at),
            eval_set_same,
            comparable_versions,
            deltas,
            warnings,
        })
    } else {
        register_baseline(
            &mut index,
            &key,
            combo,
            eval_set_sha256,
            report_path,
            rates,
            created_at,
        );
        save_index(index_path, &index)?;
        Ok(BaselineComparison {
            combo_key: key,
            is_baseline: true,
            baseline_report: None,
            baseline_created_at: None,
            eval_set_same: true,
            comparable_versions,
            deltas: Vec::new(),
            warnings: Vec::new(),
        })
    }
}

fn register_baseline(
    index: &mut BaselineIndex,
    key: &str,
    combo: &ComboInfo,
    eval_set_sha256: &str,
    report_path: &Path,
    rates: &MetricRates,
    created_at: &str,
) {
    index.entries.insert(
        key.to_string(),
        BaselineEntry {
            combo_key: key.to_string(),
            provider: combo.provider.clone(),
            model: combo.model.clone(),
            prompt_version: combo.prompt_version,
            eval_set_version: combo.eval_set_version.clone(),
            taxonomy_base_version: combo.taxonomy_base_version.clone(),
            taxonomy_effective_version: combo.taxonomy_effective_version.clone(),
            split: combo.split.clone(),
            report_path: report_path.display().to_string(),
            created_at: created_at.to_string(),
            eval_set_sha256: eval_set_sha256.to_string(),
            rates: StoredRates::from(rates),
        },
    );
}

/// 盲测集重复运行检测（#14 §4.3：同一候选版本只应跑一次，多跑视为污染）。
pub fn count_previous_runs(reports_dir: &Path, combo_key: &str, split: &str) -> usize {
    let Ok(entries) = std::fs::read_dir(reports_dir) else {
        return 0;
    };
    let mut count = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "json").unwrap_or(true) {
            continue;
        }
        if path
            .file_name()
            .map(|n| n == BASELINE_INDEX_FILE)
            .unwrap_or(false)
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if value.get("kind").and_then(|v| v.as_str()) != Some("eval") {
            continue;
        }
        if value.get("combo_key").and_then(|v| v.as_str()) == Some(combo_key)
            && value.get("split").and_then(|v| v.as_str()) == Some(split)
        {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates(l1: f64, l2: f64, f1: f64, abstain: f64, availability: f64) -> MetricRates {
        MetricRates {
            l1_accuracy: Some(l1),
            l2_accuracy: Some(l2),
            l2_given_l1: Some(l1),
            format_f1: Some(f1),
            abstain_rate: Some(abstain),
            availability: Some(availability),
        }
    }

    fn combo() -> ComboInfo {
        ComboInfo {
            provider: "dashscope".into(),
            model: "qwen-flash".into(),
            prompt_version: 1,
            eval_set_version: "2026-10-v1".into(),
            taxonomy_base_version: "1.1.0".into(),
            taxonomy_effective_version: "1.1.0+u0".into(),
            split: "visible".into(),
        }
    }

    fn temp_index() -> PathBuf {
        std::env::temp_dir()
            .join(format!(
                "aether-baseline-test-{}-{}",
                std::process::id(),
                fastrand::u64(..)
            ))
            .join(BASELINE_INDEX_FILE)
    }

    #[test]
    fn first_run_is_baseline_then_compares() {
        let path = temp_index();
        let report = path.parent().unwrap().join("report.json");
        std::fs::create_dir_all(report.parent().unwrap()).unwrap();
        std::fs::write(&report, "{}").unwrap();
        let first = compare_and_register(
            &path,
            &combo(),
            "hash1",
            &report,
            &rates(0.95, 0.90, 0.80, 0.04, 1.0),
            "2026-10-21T10:00:00+08:00",
            true,
        )
        .unwrap();
        assert!(first.is_baseline);

        let second = compare_and_register(
            &path,
            &combo(),
            "hash1",
            &report,
            &rates(0.93, 0.88, 0.77, 0.05, 1.0),
            "2026-10-21T11:00:00+08:00",
            true,
        )
        .unwrap();
        assert!(!second.is_baseline);
        let l1 = second
            .deltas
            .iter()
            .find(|d| d.key == "l1_accuracy")
            .unwrap();
        assert!((l1.delta.unwrap() + 0.02).abs() < 1e-9);
        assert!(!second.warnings.is_empty());
    }

    #[test]
    fn changed_eval_set_hash_resets_baseline() {
        let path = temp_index();
        let report = path.parent().unwrap().join("report.json");
        std::fs::create_dir_all(report.parent().unwrap()).unwrap();
        std::fs::write(&report, "{}").unwrap();
        compare_and_register(
            &path,
            &combo(),
            "hash1",
            &report,
            &rates(0.9, 0.9, 0.8, 0.05, 1.0),
            "t1",
            true,
        )
        .unwrap();
        let second = compare_and_register(
            &path,
            &combo(),
            "hash2",
            &report,
            &rates(0.9, 0.9, 0.8, 0.05, 1.0),
            "t2",
            true,
        )
        .unwrap();
        assert!(second.is_baseline);
        assert!(second.warnings.iter().any(|w| w.contains("评测集内容")));
    }

    #[test]
    fn incompatible_versions_never_compare() {
        let path = temp_index();
        let report = path.parent().unwrap().join("report.json");
        std::fs::create_dir_all(report.parent().unwrap()).unwrap();
        std::fs::write(&report, "{}").unwrap();
        compare_and_register(
            &path,
            &combo(),
            "hash1",
            &report,
            &rates(0.9, 0.9, 0.8, 0.05, 1.0),
            "t1",
            true,
        )
        .unwrap();
        let second = compare_and_register(
            &path,
            &combo(),
            "hash1",
            &report,
            &rates(0.9, 0.9, 0.8, 0.05, 1.0),
            "t2",
            false,
        )
        .unwrap();
        assert!(second.is_baseline);
        assert!(second.warnings.iter().any(|w| w.contains("版本校验未通过")));
    }
}
