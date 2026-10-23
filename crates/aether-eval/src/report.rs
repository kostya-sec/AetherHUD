//! Markdown + JSON 报告渲染（#14 §4.2）。
//!
//! 报告落 app_data_dir/eval/reports/&lt;timestamp&gt;.md/.json（只创建不覆盖），
//! 同目录附带逐条结果 .items.jsonl 与特征标签抽样 .feature-review.jsonl。

use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{json, Value};

use crate::baseline::BaselineComparison;
use crate::eval_set::{CompositionStats, VersionCheck};
use crate::metrics::{
    gate_checks, gate_overall, BoundaryMetrics, ErrorItem, EvalMetrics, FailureItem, RunItem,
};
use crate::util;

#[derive(Debug, Clone)]
pub struct ReportParams {
    pub provider: String,
    pub model: String,
    pub prompt_version: u32,
    pub template_hash: String,
    pub fewshot_hash: String,
    pub fewshot_count: usize,
    pub tag_pool_hash: String,
    pub tag_pool_count: usize,
    pub temperature: f64,
    pub max_tokens: u32,
    pub base_url: String,
    pub request_timeout_ms: u64,
    pub retry_max_attempts: u32,
    pub retry_base_secs: u64,
    pub retry_max_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ReportVersions {
    pub eval_set_version: String,
    pub eval_set_sha256: String,
    pub taxonomy_base_version: String,
    pub taxonomy_effective_version: String,
    pub taxonomy_effective_source: String,
    pub taxonomy_effective_path: Option<String>,
    pub version_check: VersionCheck,
}

#[derive(Debug, Clone)]
pub struct SetInfo {
    pub kind: String,
    pub file: String,
    pub total_file_items: usize,
    pub evaluated: usize,
    pub skipped_invalid: usize,
    pub limit: Option<usize>,
    pub parse_errors: Vec<String>,
    pub composition: CompositionStats,
    pub composition_warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RunInfo {
    pub started_at: String,
    pub finished_at: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UncoveredNode {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct FeatureReviewInfo {
    pub sampled: usize,
    pub file: String,
}

#[derive(Debug, Clone)]
pub struct ReportsContext<'a> {
    pub params: &'a ReportParams,
    pub versions: &'a ReportVersions,
    pub set: &'a SetInfo,
    pub run: &'a RunInfo,
    pub metrics: Option<&'a EvalMetrics>,
    pub boundary: Option<&'a BoundaryMetrics>,
    pub baseline: &'a BaselineComparison,
    pub uncovered_l2: &'a [UncoveredNode],
    pub warnings: Vec<String>,
    pub feature_review: Option<FeatureReviewInfo>,
    pub items: &'a [RunItem],
}

#[derive(Debug, Clone)]
pub struct ReportPaths {
    pub json: PathBuf,
    pub md: PathBuf,
    pub items_jsonl: PathBuf,
    pub feature_review: Option<PathBuf>,
}

#[derive(Debug, thiserror::Error)]
pub enum ReportError {
    #[error("写入报告失败（{path}）：{source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("序列化报告失败：{0}")]
    Json(String),
}

pub fn report_paths(stem: &Path) -> ReportPaths {
    ReportPaths {
        json: stem.with_extension("json"),
        md: stem.with_extension("md"),
        items_jsonl: stem.with_extension("items.jsonl"),
        feature_review: None,
    }
}

pub fn write_reports(
    mut paths: ReportPaths,
    ctx: &ReportsContext<'_>,
) -> Result<ReportPaths, ReportError> {
    let json_value = build_json(ctx, &paths);
    let json_text =
        serde_json::to_string_pretty(&json_value).map_err(|e| ReportError::Json(e.to_string()))?;
    let mut md = render_markdown(ctx);
    if let Some(review) = &ctx.feature_review {
        paths.feature_review = Some(PathBuf::from(&review.file));
        md.push_str(&format!(
            "\n> 特征标签抽样 {} 条已写入：{}（回填 verdict 后统计离谱率）\n",
            review.sampled, review.file
        ));
    }
    write(&paths.json, &json_text)?;
    write(&paths.md, &md)?;
    write(&paths.items_jsonl, &render_items_jsonl(ctx.items))?;
    if ctx.metrics.is_some() {
        if let Some(review) = &ctx.feature_review {
            let (sample, text) = render_feature_review(ctx.items);
            debug_assert_eq!(sample, review.sampled);
            write(&PathBuf::from(&review.file), &text)?;
        }
    }
    Ok(paths)
}

fn write(path: &Path, text: &str) -> Result<(), ReportError> {
    util::write_new(path, text).map_err(|source| ReportError::Write {
        path: path.display().to_string(),
        source,
    })
}

fn build_json(ctx: &ReportsContext<'_>, paths: &ReportPaths) -> Value {
    let params = ctx.params;
    let versions = ctx.versions;
    let check = &versions.version_check;
    let rates = ctx.metrics.map(|m| m.rates());
    let checks = rates.as_ref().map(gate_checks).unwrap_or_default();
    let gate_overall = gate_overall(&checks);
    let gate_value = match gate_overall {
        Some(true) => json!("PASS"),
        Some(false) => json!("FAIL"),
        None => json!("—"),
    };
    let baseline = json!({
        "combo_key": ctx.baseline.combo_key,
        "is_baseline": ctx.baseline.is_baseline,
        "baseline_report": ctx.baseline.baseline_report,
        "baseline_created_at": ctx.baseline.baseline_created_at,
        "eval_set_same": ctx.baseline.eval_set_same,
        "comparable_versions": ctx.baseline.comparable_versions,
        "deltas": ctx.baseline.deltas,
        "warnings": ctx.baseline.warnings,
    });
    let mut value = json!({
        "schema": "aether-eval-report/v1",
        "kind": "eval",
        "combo_key": ctx.baseline.combo_key,
        "split": ctx.set.kind,
        "generated_at": ctx.run.finished_at,
        "duration_ms": ctx.run.duration_ms,
        "report": {
            "json": paths.json.display().to_string(),
            "markdown": paths.md.display().to_string(),
        },
        "params": {
            "provider": params.provider,
            "model": params.model,
            "prompt_version": params.prompt_version,
            "template_hash": params.template_hash,
            "fewshot_hash": params.fewshot_hash,
            "fewshot_count": params.fewshot_count,
            "tag_pool_hash": params.tag_pool_hash,
            "tag_pool_count": params.tag_pool_count,
            "temperature": params.temperature,
            "max_tokens": params.max_tokens,
            "base_url": params.base_url,
            "request_timeout_ms": params.request_timeout_ms,
            "retry": {
                "max_attempts": params.retry_max_attempts,
                "base_secs": params.retry_base_secs,
                "max_secs": params.retry_max_secs,
            },
        },
        "versions": {
            "eval_set_version": versions.eval_set_version,
            "eval_set_sha256": versions.eval_set_sha256,
            "taxonomy_base_version": versions.taxonomy_base_version,
            "taxonomy_effective_version": versions.taxonomy_effective_version,
            "taxonomy_effective_source": versions.taxonomy_effective_source,
            "taxonomy_effective_path": versions.taxonomy_effective_path,
            "eval_set_taxonomy_version": check.eval_set_taxonomy_version,
            "base_version_matches": check.base_version_matches,
            "effective_version_matches": check.effective_version_matches,
            "comparable": check.comparable,
            "notes": check.notes,
        },
        "set": {
            "kind": ctx.set.kind,
            "file": ctx.set.file,
            "total_file_items": ctx.set.total_file_items,
            "evaluated": ctx.set.evaluated,
            "skipped_invalid": ctx.set.skipped_invalid,
            "limit": ctx.set.limit,
            "parse_errors": ctx.set.parse_errors,
            "composition": ctx.set.composition,
            "composition_warnings": ctx.set.composition_warnings,
        },
        "run": {
            "started_at": ctx.run.started_at,
            "finished_at": ctx.run.finished_at,
            "duration_ms": ctx.run.duration_ms,
        },
        "baseline": baseline,
        "uncovered_l2": ctx.uncovered_l2,
        "gate": {
            "overall": gate_value,
            "checks": checks,
        },
        "warnings": ctx.warnings,
    });
    if let Some(metrics) = ctx.metrics {
        value["metrics"] = serde_json::to_value(metrics).unwrap_or(Value::Null);
        value["gate"]["caveat"] = if ctx.set.composition_warnings.is_empty() {
            json!(null)
        } else {
            json!("评测集分层存在缺口，门线判定仅供参考（#14 §3.1）")
        };
    }
    if let Some(boundary) = ctx.boundary {
        value["boundary"] = serde_json::to_value(boundary).unwrap_or(Value::Null);
    }
    if let Some(review) = &ctx.feature_review {
        value["feature_review"] = json!({
            "sampled": review.sampled,
            "file": review.file,
            "metric": "特征标签离谱率：人工回填 verdict=bad 的比例（只观察，不设门线）",
        });
    }
    value
}

pub fn render_items_jsonl(items: &[RunItem]) -> String {
    let mut out = String::new();
    for run in items {
        let gold = run.gold.as_ref().map(|g| {
            json!({
                "l1_id": g.l1_id,
                "l2_id": g.l2_id,
                "format_tags": g.format_tags,
                "feature_tags": g.feature_tags,
            })
        });
        let status = match run.outcome.status {
            aether_classifier::OutcomeStatus::Ok => "ok",
            aether_classifier::OutcomeStatus::FormatInvalid => "format_invalid",
            aether_classifier::OutcomeStatus::Unavailable => "unavailable",
        };
        let output = run.outcome.classification.as_ref().map(|c| {
            json!({
                "l1_id": c.l1_id,
                "l2_id": c.l2_id,
                "format_tags": c.format_tags,
                "feature_tags": c.feature_tags,
                "suggestion": c.suggestion,
                "abstained": run.outcome.abstained,
            })
        });
        let row = json!({
            "line": run.line,
            "entry_id": run.item.entry_id,
            "title": run.item.title,
            "url": run.item.url,
            "media_type": run.item.media_type,
            "difficulty": run.item.difficulty,
            "split": run.item.split,
            "gold": gold,
            "status": status,
            "output": output,
            "raw_output": run.outcome.raw_output,
            "first_raw_output": run.outcome.first_raw_output,
            "failure_reason": run.outcome.failure_reason,
            "repair_trigger_issues": run.outcome.repair_trigger_issues,
            "repair_used": run.outcome.repair_used,
            "issues": run.outcome.issues,
            "elapsed_ms": run.outcome.elapsed_ms,
            "usage": run.outcome.usage,
            "calls": run.outcome.calls,
        });
        out.push_str(&serde_json::to_string(&row).unwrap_or_default());
        out.push('\n');
    }
    out
}

pub fn sample_feature_review(items: &[RunItem]) -> Vec<&RunItem> {
    let mut classified: Vec<&RunItem> = items
        .iter()
        .filter(|item| item.outcome.status == aether_classifier::OutcomeStatus::Ok)
        .collect();
    classified.sort_by_key(|item| util::sha256_hex(item.item.entry_id.as_bytes()));
    classified.truncate(20);
    classified
}

fn render_feature_review(items: &[RunItem]) -> (usize, String) {
    let sample = sample_feature_review(items);
    let mut out = String::new();
    for run in &sample {
        let gold = run
            .gold
            .as_ref()
            .map(|g| g.feature_tags.clone())
            .unwrap_or_default();
        let model = run
            .outcome
            .classification
            .as_ref()
            .map(|c| c.feature_tags.clone())
            .unwrap_or_default();
        let row = json!({
            "entry_id": run.item.entry_id,
            "title": run.item.title,
            "difficulty": run.item.difficulty,
            "gold_feature_tags": gold,
            "model_feature_tags": model,
            "verdict": "",
            "note": "",
        });
        out.push_str(&serde_json::to_string(&row).unwrap_or_default());
        out.push('\n');
    }
    (sample.len(), out)
}

fn render_markdown(ctx: &ReportsContext<'_>) -> String {
    let mut out = String::new();
    out.push_str(&format!("# AetherHUD 分类评测报告（{}）\n\n", ctx.set.kind));
    out.push_str(&format!(
        "> 生成时间：{} ｜ 运行耗时 {} ｜ 评测集：{} ｜ provider/model：{} / {} ｜ prompt_version：{}\n",
        ctx.run.finished_at,
        util::fmt_duration_ms(ctx.run.duration_ms),
        ctx.versions.eval_set_version,
        ctx.params.provider,
        ctx.params.model,
        ctx.params.prompt_version
    ));
    out.push_str("> 报告与评测集只存本地，不入 git（#14 §3.4）；门线口径见 #14 §2.4。\n\n");

    out.push_str("## 1. 参数与版本\n\n");
    let mut rows = vec![
        vec![
            "eval_set_version".to_string(),
            ctx.versions.eval_set_version.clone(),
        ],
        vec![
            "taxonomy 底表版本".to_string(),
            ctx.versions.taxonomy_base_version.clone(),
        ],
        vec![
            "taxonomy 生效版本".to_string(),
            ctx.versions.taxonomy_effective_version.clone(),
        ],
        vec![
            "生效树来源".to_string(),
            ctx.versions.taxonomy_effective_source.clone(),
        ],
        vec![
            "provider / model".to_string(),
            format!("{} / {}", ctx.params.provider, ctx.params.model),
        ],
        vec![
            "prompt_version / template_hash".to_string(),
            format!(
                "{} / {}",
                ctx.params.prompt_version, ctx.params.template_hash
            ),
        ],
        vec![
            "few-shot".to_string(),
            format!(
                "{} 条（hash {}）",
                ctx.params.fewshot_count, ctx.params.fewshot_hash
            ),
        ],
        vec![
            "特征标签候选池".to_string(),
            format!(
                "{} 个（hash {}）",
                ctx.params.tag_pool_count, ctx.params.tag_pool_hash
            ),
        ],
        vec![
            "temperature / max_tokens".to_string(),
            format!("{} / {}", ctx.params.temperature, ctx.params.max_tokens),
        ],
        vec![
            "重试策略".to_string(),
            format!(
                "#11：最多 {} 次，退避 {}s×2^n±20%，封顶 {}s",
                ctx.params.retry_max_attempts,
                ctx.params.retry_base_secs,
                ctx.params.retry_max_secs
            ),
        ],
        vec!["评测集文件".to_string(), ctx.set.file.clone()],
        vec![
            "条目数".to_string(),
            format!(
                "全量 {} 条，本次 {} 条（跳过金标非法 {} 条）",
                ctx.set.total_file_items, ctx.set.evaluated, ctx.set.skipped_invalid
            ),
        ],
        vec![
            "eval_set 文件哈希".to_string(),
            ctx.versions.eval_set_sha256.clone(),
        ],
    ];
    if let Some(limit) = ctx.set.limit {
        rows.push(vec!["--limit".to_string(), limit.to_string()]);
    }
    out.push_str(&table(&["项", "值"], rows));
    out.push('\n');

    if !ctx.versions.version_check.notes.is_empty() {
        out.push_str("**版本校验**\n\n");
        for note in &ctx.versions.version_check.notes {
            out.push_str(&format!("- {note}\n"));
        }
        out.push('\n');
    }

    if let Some(metrics) = ctx.metrics {
        render_main_metrics(ctx, metrics, &mut out);
    }
    if let Some(boundary) = ctx.boundary {
        render_boundary(boundary, &mut out);
    }
    render_composition(ctx, &mut out);
    if !ctx.uncovered_l2.is_empty() {
        out.push_str("## 未覆盖节点\n\n");
        out.push_str("以下自建 L2 本次没有任何金标样本，不参与门线（#18 §8.3）：\n\n");
        for node in ctx.uncovered_l2 {
            out.push_str(&format!("- {}（{}）\n", node.name, node.id));
        }
        out.push('\n');
    }
    render_baseline(ctx, &mut out);
    if !ctx.warnings.is_empty() {
        out.push_str("## 警告\n\n");
        for warning in &ctx.warnings {
            out.push_str(&format!("- {warning}\n"));
        }
        out.push('\n');
    }
    out
}

fn render_main_metrics(ctx: &ReportsContext<'_>, metrics: &EvalMetrics, out: &mut String) {
    let rates = metrics.rates();
    out.push_str("## 2. 总体指标（#14 §2.3）\n\n");
    out.push_str(&format!(
        "评测条目 {}：成功分类 {}，分类失败 {}（格式非法 {} / API 不可用 {}），弃权 {}。\n\n",
        metrics.total_evaluated,
        metrics.classified,
        metrics.failed,
        metrics.format_failed,
        metrics.unavailable,
        metrics.abstained
    ));
    let rows = vec![
        vec![
            "L1 准确率".to_string(),
            util::fmt_pct(rates.l1_accuracy),
            format!("{}/{}", metrics.l1_correct, metrics.classified),
        ],
        vec![
            "L2 准确率".to_string(),
            util::fmt_pct(rates.l2_accuracy),
            format!("{}/{}", metrics.l2_correct, metrics.classified),
        ],
        vec![
            "L2|L1 条件准确率".to_string(),
            util::fmt_pct(rates.l2_given_l1),
            format!("{}/{}", metrics.l2_given_l1_correct, metrics.l1_correct),
        ],
        vec![
            "形态标签 micro-F1".to_string(),
            util::fmt_rate(rates.format_f1),
            format!(
                "TP {} / FP {} / FN {}",
                metrics.format_tp, metrics.format_fp, metrics.format_fn
            ),
        ],
        vec![
            "弃权率".to_string(),
            util::fmt_pct(rates.abstain_rate),
            format!(
                "{}/{}（成功分类分母）",
                metrics.abstained, metrics.classified
            ),
        ],
        vec![
            "分类可用性".to_string(),
            util::fmt_pct(rates.availability),
            format!(
                "{}/{}",
                metrics.total_evaluated - metrics.failed,
                metrics.total_evaluated
            ),
        ],
    ];
    out.push_str(&table(&["指标", "实测", "明细"], rows));
    out.push('\n');
    if !metrics.repair_issues.is_empty() {
        out.push_str(&format!(
            "> 修复重试触发原因：{}。\n\n",
            metrics
                .repair_issues
                .iter()
                .map(|(code, count)| format!("{code} × {count}"))
                .collect::<Vec<_>>()
                .join("、")
        ));
    }
    out.push_str(&format!(
        "> 成本：输入 {} tokens / 输出 {} tokens；API 调用 {} 次（HTTP 重试 {} 次，修复重试 {} 条）；平均每条 {}。\n\n",
        metrics.usage.prompt_tokens,
        metrics.usage.completion_tokens,
        metrics.api_calls,
        metrics.http_retries,
        metrics.repaired_items,
        if metrics.total_evaluated == 0 {
            "—".to_string()
        } else {
            util::fmt_duration_ms(metrics.latency_ms_total / metrics.total_evaluated as u64)
        }
    ));

    out.push_str("## 3. 门线判定（#14 §2.4）\n\n");
    let checks = gate_checks(&rates);
    let overall = match gate_overall(&checks) {
        Some(true) => "PASS",
        Some(false) => "FAIL",
        None => "—",
    };
    out.push_str(&format!("总体门线：**{overall}**\n\n"));
    let mut gate_rows = Vec::new();
    for check in &checks {
        let target = if check.higher_is_better {
            format!("≥ {}", util::fmt_pct(Some(check.target)))
        } else {
            format!("≤ {}", util::fmt_pct(Some(check.target)))
        };
        let floor = if check.higher_is_better {
            format!("≥ {}", util::fmt_pct(Some(check.floor)))
        } else {
            format!("≤ {}", util::fmt_pct(Some(check.floor)))
        };
        let actual = if check.key == "format_f1" {
            util::fmt_rate(check.actual)
        } else {
            util::fmt_pct(check.actual)
        };
        let pass = match check.floor_pass {
            Some(true) => "✅ 过",
            Some(false) => "❌ 未过",
            None => "—",
        };
        gate_rows.push(vec![
            check.label.clone(),
            actual,
            target,
            floor,
            pass.to_string(),
        ]);
    }
    out.push_str(&table(
        &["指标", "实测", "目标值", "发布门线", "判定"],
        gate_rows,
    ));
    if !ctx.set.composition_warnings.is_empty() {
        out.push_str("\n> ⚠ 评测集分层存在缺口（见“评测集组成”），门线判定仅供参考。\n");
    }
    out.push('\n');

    out.push_str("## 4. 分组指标（仅观察，不单独设门槛）\n\n");
    out.push_str("### 按金标 L1\n\n");
    let mut rows = Vec::new();
    for (l1, group) in &metrics.by_l1 {
        rows.push(vec![
            l1.clone(),
            group.total.to_string(),
            group.classified.to_string(),
            group.failed.to_string(),
            util::fmt_pct(group.l1_accuracy()),
            util::fmt_pct(group.l2_accuracy()),
            util::fmt_pct(group.abstain_rate()),
        ]);
    }
    out.push_str(&table(
        &[
            "金标 L1",
            "条目",
            "成功分类",
            "失败",
            "L1 准确率",
            "L2 准确率",
            "弃权率",
        ],
        rows,
    ));
    out.push_str("\n### 按金标 L2\n\n");
    let mut rows = Vec::new();
    for (l2, group) in &metrics.by_l2 {
        rows.push(vec![
            l2.clone(),
            group.total.to_string(),
            group.classified.to_string(),
            util::fmt_pct(group.l1_accuracy()),
            util::fmt_pct(group.l2_accuracy()),
            util::fmt_pct(group.abstain_rate()),
        ]);
    }
    out.push_str(&table(
        &[
            "金标 L2",
            "条目",
            "成功分类",
            "L1 准确率",
            "L2 准确率",
            "弃权率",
        ],
        rows,
    ));
    out.push_str("\n### 按难度\n\n");
    let mut rows = Vec::new();
    for (difficulty, group) in &metrics.by_difficulty {
        rows.push(vec![
            difficulty.clone(),
            group.total.to_string(),
            util::fmt_pct(group.l1_accuracy()),
            util::fmt_pct(group.l2_accuracy()),
            util::fmt_pct(group.abstain_rate()),
        ]);
    }
    out.push_str(&table(
        &["难度", "条目", "L1 准确率", "L2 准确率", "弃权率"],
        rows,
    ));
    out.push('\n');

    out.push_str("## 5. 混淆矩阵（金标 L1 → 输出 L1）\n\n");
    let mut columns: Vec<String> = metrics.confusion.keys().cloned().collect();
    columns.sort();
    let mut matrix_rows = Vec::new();
    for gold_l1 in &columns {
        let mut row = vec![gold_l1.clone()];
        for pred_l1 in &columns {
            row.push(
                metrics
                    .confusion
                    .get(gold_l1)
                    .and_then(|m| m.get(pred_l1))
                    .copied()
                    .unwrap_or(0)
                    .to_string(),
            );
        }
        matrix_rows.push(row);
    }
    let mut headers = vec!["金标 \\ 输出".to_string()];
    headers.extend(columns.iter().cloned());
    let header_refs: Vec<&str> = headers.iter().map(String::as_str).collect();
    out.push_str(&table(&header_refs, matrix_rows));
    out.push('\n');

    out.push_str("## 6. 错例 Top 20\n\n");
    if metrics.errors.is_empty() {
        out.push_str("无错例。\n\n");
    } else {
        let mut rows = Vec::new();
        for error in metrics.errors.iter().take(20) {
            rows.push(error_row(error));
        }
        out.push_str(&table(
            &["#", "标题", "难度", "金标 L1/L2", "输出 L1/L2", "原因"],
            rows,
        ));
        out.push_str(&format!(
            "\n> 共 {} 条错例（弃权计入 L1/L2 错误，但单列弃权率）。完整列表见 JSON 报告。\n\n",
            metrics.errors.len()
        ));
    }

    out.push_str("## 7. 失败条目（不计入准确率分母）\n\n");
    if metrics.failures.is_empty() {
        out.push_str("无失败条目。\n\n");
    } else {
        let mut rows = Vec::new();
        for failure in &metrics.failures {
            rows.push(failure_row(failure));
        }
        out.push_str(&table(&["行", "标题", "难度", "类型", "原因"], rows));
        out.push('\n');
    }
}

fn error_row(error: &ErrorItem) -> Vec<String> {
    vec![
        error.line.to_string(),
        util::truncate_chars(&error.title, 30),
        error.difficulty.clone(),
        format!("{} / {}", error.gold_l1, error.gold_l2),
        format!("{} / {}", error.pred_l1, error.pred_l2),
        match error.kind.as_str() {
            "abstain" => "弃权".to_string(),
            "l1_wrong" => "L1 错误".to_string(),
            "l2_wrong" => "L2 错误".to_string(),
            other => other.to_string(),
        },
    ]
}

fn failure_row(failure: &FailureItem) -> Vec<String> {
    vec![
        failure.line.to_string(),
        util::truncate_chars(&failure.title, 30),
        failure.difficulty.clone(),
        failure.status.clone(),
        util::truncate_chars(&failure.reason, 60),
    ]
}

fn render_boundary(boundary: &BoundaryMetrics, out: &mut String) {
    out.push_str("## 边界集观察（不参与门线，不计入 200 条）\n\n");
    out.push_str(&format!(
        "条目 {}：成功分类 {}（弃权 {}），失败 {}（格式非法 {} / API 不可用 {}）。\n\n",
        boundary.total,
        boundary.classified,
        boundary.abstained,
        boundary.failed,
        boundary.format_failed,
        boundary.unavailable
    ));
    out.push_str("### 输出分布（成功分类）\n\n");
    let mut rows = Vec::new();
    for (l1, count) in &boundary.by_pred_l1 {
        rows.push(vec![l1.clone(), count.to_string()]);
    }
    out.push_str(&table(&["输出 L1", "条数"], rows));
    out.push('\n');
    let mut rows = Vec::new();
    for (l2, count) in &boundary.by_pred_l2 {
        rows.push(vec![l2.clone(), count.to_string()]);
    }
    out.push_str(&table(&["输出 L2", "条数"], rows));
    out.push('\n');
    out.push_str("### 分类建议（#18 §2.2，不计分）\n\n");
    out.push_str(&format!(
        "弃权 {} 条中，带 suggestion {} 条；nearest_l2_id {} 条；new_l2 {} 条。\n\n",
        boundary.abstained,
        boundary.with_suggestion,
        boundary.with_nearest_l2,
        boundary.with_new_l2
    ));
    if !boundary.new_l2_by_parent.is_empty() {
        let mut rows = Vec::new();
        for (parent, count) in &boundary.new_l2_by_parent {
            rows.push(vec![parent.clone(), count.to_string()]);
        }
        out.push_str(&table(&["建议新建的父 L1", "条数"], rows));
        out.push('\n');
    }
    let mut rows = Vec::new();
    for row in &boundary.rows {
        let suggestion = row
            .suggestion
            .as_ref()
            .map(|s| util::truncate_chars(&s.to_string(), 60))
            .unwrap_or_default();
        rows.push(vec![
            row.line.to_string(),
            util::truncate_chars(&row.title, 28),
            if row.abstained {
                "弃权".to_string()
            } else {
                format!("{} / {}", row.pred_l1, row.pred_l2)
            },
            row.status.clone(),
            suggestion,
        ]);
    }
    out.push_str(&table(&["行", "标题", "输出", "状态", "suggestion"], rows));
    out.push('\n');
}

fn render_composition(ctx: &ReportsContext<'_>, out: &mut String) {
    out.push_str("## 评测集组成（全量文件）\n\n");
    let stats = &ctx.set.composition;
    out.push_str(&format!(
        "总量 {} 条；split：{}。\n\n",
        stats.total,
        stats
            .splits
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(" / ")
    ));
    let mut rows = Vec::new();
    for (l1, count) in &stats.by_l1 {
        rows.push(vec![l1.clone(), count.to_string()]);
    }
    out.push_str(&table(&["金标 L1", "条数"], rows));
    out.push('\n');
    let mut rows = Vec::new();
    for (l2, count) in &stats.by_l2 {
        rows.push(vec![l2.clone(), count.to_string()]);
    }
    out.push_str(&table(&["金标 L2", "条数"], rows));
    out.push('\n');
    let mut rows = Vec::new();
    for (kind, count) in &stats.media {
        rows.push(vec![kind.clone(), count.to_string()]);
    }
    out.push_str(&table(&["媒体形态", "条数"], rows));
    out.push('\n');
    if !ctx.set.composition_warnings.is_empty() {
        for warning in &ctx.set.composition_warnings {
            out.push_str(&format!("- ⚠ {warning}\n"));
        }
        out.push('\n');
    }
}

fn render_baseline(ctx: &ReportsContext<'_>, out: &mut String) {
    out.push_str("## 基线对比（#14 §4.4）\n\n");
    out.push_str(&format!("组合 key：{}\n\n", ctx.baseline.combo_key));
    if ctx.baseline.is_baseline {
        out.push_str(
            "本次运行为该组合的**首个基线**（基线报告不可覆盖；后续同条件运行与它比较）。\n",
        );
        if let Some(path) = &ctx.baseline.baseline_report {
            out.push_str(&format!("基线报告：{path}\n"));
        }
        out.push('\n');
    } else {
        out.push_str(&format!(
            "与基线报告（{}）比较：\n\n",
            ctx.baseline.baseline_report.clone().unwrap_or_default()
        ));
        let mut rows = Vec::new();
        for delta in &ctx.baseline.deltas {
            let fmt = |v: Option<f64>| {
                if delta.as_percent {
                    util::fmt_pct(v)
                } else {
                    util::fmt_rate(v)
                }
            };
            rows.push(vec![
                delta.label.clone(),
                fmt(delta.baseline),
                fmt(delta.current),
                util::fmt_delta(delta.delta, delta.as_percent),
            ]);
        }
        out.push_str(&table(&["指标", "基线", "本次", "差值"], rows));
        out.push_str("\n> 门线硬约束之外，任一主指标下降 > 2pt 应回滚本次改动（#14 §2.5）。\n\n");
    }
    for warning in &ctx.baseline.warnings {
        out.push_str(&format!("- {warning}\n"));
    }
    out.push('\n');
}

fn table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let mut out = String::new();
    out.push_str("| ");
    out.push_str(&headers.join(" | "));
    out.push_str(" |\n|");
    for _ in headers {
        out.push_str(" --- |");
    }
    out.push('\n');
    for row in rows {
        out.push_str("| ");
        let cells: Vec<String> = row.iter().map(|cell| escape_cell(cell)).collect();
        out.push_str(&cells.join(" | "));
        out.push_str(" |\n");
    }
    out
}

fn escape_cell(cell: &str) -> String {
    cell.chars()
        .map(|ch| match ch {
            '|' => "\\|".to_string(),
            '\n' | '\r' => " ".to_string(),
            other => other.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_classifier::engine::ClassificationOutcome;
    use aether_classifier::provider::Usage;
    use aether_classifier::schema::Classification;
    use aether_classifier::OutcomeStatus;

    fn run_item(wrong: bool) -> RunItem {
        RunItem {
            line: 1,
            item: crate::eval_set::EvalItem {
                entry_id: "01A".to_string(),
                title: "标题".to_string(),
                url: String::new(),
                body_excerpt: String::new(),
                media_type: "link".to_string(),
                gold: None,
                difficulty: Some("easy".to_string()),
                split: Some("visible".to_string()),
                eval_set_version: Some("2026-10-v1".to_string()),
                taxonomy_version: Some("1.1.0".to_string()),
                notes: None,
            },
            gold: Some(crate::eval_set::GoldLabel {
                l1_id: "l1_tech".to_string(),
                l2_id: "l2_coding".to_string(),
                format_tags: vec!["tag_tool".to_string()],
                feature_tags: vec![],
            }),
            remapped_from: None,
            outcome: ClassificationOutcome {
                status: OutcomeStatus::Ok,
                classification: Some(Classification {
                    l1_id: if wrong { "l1_biz" } else { "l1_tech" }.to_string(),
                    l2_id: if wrong { "l2_insight" } else { "l2_coding" }.to_string(),
                    format_tags: vec!["tag_tool".to_string()],
                    feature_tags: vec![],
                    suggestion: None,
                }),
                abstained: false,
                raw_output: None,
                first_raw_output: None,
                repair_used: false,
                issues: Vec::new(),
                repair_trigger_issues: Vec::new(),
                failure_reason: None,
                calls: Vec::new(),
                usage: Usage::default(),
                elapsed_ms: 1,
            },
        }
    }

    fn dummy_tree() -> aether_classifier::taxonomy::TaxonomyView {
        aether_classifier::taxonomy::base_view_from_json(
            r#"{"version":"1.1.0","global_l3_tags":{"format_tags":[{"id":"tag_tool","name":"工具"}]},
            "taxonomy":[
              {"id":"l1_inbox","name":"00 未分类","children":[{"id":"l2_manual_review","name":"待人工归档"}]},
              {"id":"l1_tech","name":"03 技术与工程","children":[{"id":"l2_coding","name":"编程与低代码"}]},
              {"id":"l1_biz","name":"04 商业与认知","children":[{"id":"l2_insight","name":"行业观察与研报"}]}
            ]}"#,
        )
        .unwrap()
    }

    fn params() -> ReportParams {
        ReportParams {
            provider: "dashscope".into(),
            model: "qwen-flash".into(),
            prompt_version: 1,
            template_hash: "abcd1234".into(),
            fewshot_hash: "1234abcd".into(),
            fewshot_count: 8,
            tag_pool_hash: "ffff".into(),
            tag_pool_count: 26,
            temperature: 0.0,
            max_tokens: 400,
            base_url: "https://example.com/v1".into(),
            request_timeout_ms: 30000,
            retry_max_attempts: 8,
            retry_base_secs: 30,
            retry_max_secs: 1800,
        }
    }

    fn versions() -> ReportVersions {
        ReportVersions {
            eval_set_version: "2026-10-v1".into(),
            eval_set_sha256: "sha".into(),
            taxonomy_base_version: "1.1.0".into(),
            taxonomy_effective_version: "1.1.0+u0".into(),
            taxonomy_effective_source: "file".into(),
            taxonomy_effective_path: None,
            version_check: VersionCheck {
                eval_set_version: Some("2026-10-v1".into()),
                eval_set_version_consistent: true,
                eval_set_taxonomy_version: Some("1.1.0".into()),
                base_version_matches: Some(true),
                effective_version_matches: Some(true),
                comparable: true,
                notes: vec![],
            },
        }
    }

    fn set_info() -> SetInfo {
        SetInfo {
            kind: "visible".into(),
            file: "x.jsonl".into(),
            total_file_items: 200,
            evaluated: 2,
            skipped_invalid: 0,
            limit: None,
            parse_errors: vec![],
            composition: CompositionStats::default(),
            composition_warnings: vec![],
        }
    }

    #[test]
    fn renders_required_sections_and_json_fields() {
        let items = vec![run_item(false), run_item(true)];
        let metrics = crate::metrics::compute(&items, &dummy_tree());
        let baseline = BaselineComparison::new_baseline("combo", true);
        let p = params();
        let v = versions();
        let s = set_info();
        let run = RunInfo {
            started_at: "t0".into(),
            finished_at: "t1".into(),
            duration_ms: 1000,
        };
        let ctx = ReportsContext {
            params: &p,
            versions: &v,
            set: &s,
            run: &run,
            metrics: Some(&metrics),
            boundary: None,
            baseline: &baseline,
            uncovered_l2: &[],
            warnings: vec![],
            feature_review: None,
            items: &items,
        };
        let md = render_markdown(&ctx);
        assert!(md.contains("总体指标"));
        assert!(md.contains("门线判定"));
        assert!(md.contains("混淆矩阵"));
        assert!(md.contains("错例 Top 20"));
        assert!(md.contains("基线对比"));
        assert!(md.contains("评测集组成"));
        // #27：报告必须同时回显底表版本与生效分类树版本（#18 §8.4）。
        assert!(md.contains("taxonomy 底表版本"));
        assert!(md.contains("1.1.0"));
        assert!(md.contains("taxonomy 生效版本"));
        assert!(md.contains("1.1.0+u0"));
        let json = build_json(&ctx, &report_paths(Path::new("x/report")));
        assert_eq!(json["kind"], "eval");
        assert_eq!(json["split"], "visible");
        assert_eq!(json["metrics"]["classified"], 2);
        assert!(json["gate"]["checks"].is_array());
        assert_eq!(json["combo_key"], "combo");
    }

    #[test]
    fn write_reports_creates_four_files_without_overwrite() {
        let items = vec![run_item(false)];
        let metrics = crate::metrics::compute(&items, &dummy_tree());
        let baseline = BaselineComparison::new_baseline("combo", true);
        let p = params();
        let v = versions();
        let s = set_info();
        let run = RunInfo {
            started_at: "a".into(),
            finished_at: "b".into(),
            duration_ms: 10,
        };
        let dir = std::env::temp_dir().join(format!(
            "aether-report-out-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        let stem = dir.join("20261021-120000");
        let review_file = dir.join("review.jsonl");
        let ctx = ReportsContext {
            params: &p,
            versions: &v,
            set: &s,
            run: &run,
            metrics: Some(&metrics),
            boundary: None,
            baseline: &baseline,
            uncovered_l2: &[],
            warnings: vec![],
            feature_review: Some(FeatureReviewInfo {
                sampled: 1,
                file: review_file.display().to_string(),
            }),
            items: &items,
        };
        let paths = write_reports(report_paths(&stem), &ctx).unwrap();
        assert!(paths.json.is_file());
        assert!(paths.md.is_file());
        assert!(paths.items_jsonl.is_file());
        assert!(review_file.is_file());
        assert!(write_reports(report_paths(&stem), &ctx).is_err());
    }
}
