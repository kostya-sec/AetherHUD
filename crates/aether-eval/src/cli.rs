//! aether-eval CLI：eval（默认子命令语义）/ candidates / dedupe。
//!
//! 规格：#14 §4.1（eval CLI）、#14 §6.1（candidates）、#19 §9（few-shot 查重）。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::{Args, Parser, Subcommand};
use serde_json::Map;

use aether_classifier::config::{load_few_shots, load_tag_pool};
use aether_classifier::engine::ClassificationEngine;
use aether_classifier::provider::{ChatClient, ProviderConfig};
use aether_classifier::retry::RetryPolicy;
use aether_classifier::taxonomy::{load_effective_taxonomy, EffectiveSource};
use aether_classifier::{EntryForClassify, PromptConfig};

use crate::appdata::{resolve_data_dir, resolve_eval_set_path, SetKind};
use crate::baseline::BASELINE_INDEX_FILE;
use crate::baseline::{compare_and_register, count_previous_runs, ComboInfo};
use crate::candidates::generate_candidates;
use crate::dedupe::{check_dedupe, render_text as render_dedupe_text};
use crate::eval_set::{
    check_versions, composition, composition_warnings, load_eval_set, prepare_items, PreparedItem,
};
use crate::metrics::{compute, compute_boundary, gate_checks, gate_overall, rates_table, RunItem};
use crate::report::{
    report_paths, sample_feature_review, write_reports, FeatureReviewInfo, ReportParams,
    ReportVersions, ReportsContext, RunInfo, SetInfo, UncoveredNode,
};
use crate::util;

#[derive(Parser, Debug)]
#[command(
    name = "aether-eval",
    version,
    about = "AetherHUD 分类质量评测 CLI（#21）：金标集评测、报告、纠偏候选与 few-shot 查重"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
    #[command(flatten)]
    pub eval: EvalArgs,
    /// app_data_dir（默认平台数据目录/AetherHUD；也可用 AETHER_DATA_DIR）
    #[arg(long, global = true, value_name = "DIR")]
    pub data_dir: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 从 corrections + entries 生成纠偏候选 JSONL（#14 §6.1 / #18 §8.4）
    Candidates(CandidatesArgs),
    /// few-shot 池与评测集样本级查重（#14 §6.2 / #19 §9）
    Dedupe(DedupeArgs),
}

#[derive(Args, Debug, Default)]
pub struct EvalArgs {
    /// visible / blind / boundary（必填）
    #[arg(long)]
    pub set: Option<String>,
    /// 分类 provider（默认 dashscope，报告参数）
    #[arg(long)]
    pub provider: Option<String>,
    /// 模型名（必填，如 qwen-flash）
    #[arg(long)]
    pub model: Option<String>,
    /// prompt 版本（单调递增整数，必填）
    #[arg(long = "prompt-version")]
    pub prompt_version: Option<u32>,
    /// 只跑前 N 条（成本控制；按文件顺序）
    #[arg(long)]
    pub limit: Option<usize>,
    /// 自定义报告路径前缀（默认 app_data_dir/eval/reports/<timestamp>）
    #[arg(long)]
    pub report: Option<PathBuf>,
    /// 评测集版本（默认自动发现 eval/sets 下唯一集合）
    #[arg(long = "eval-set-version")]
    pub eval_set_version: Option<String>,
    /// 直接指定评测集 JSONL
    #[arg(long = "set-file")]
    pub set_file: Option<PathBuf>,
    /// few-shot 配置路径（默认 app_data_dir/classifier/fewshot.json）
    #[arg(long)]
    pub fewshot: Option<PathBuf>,
    /// 特征标签候选池快照（默认 app_data_dir/classifier/tag_pool.json，可缺省）
    #[arg(long = "tag-pool")]
    pub tag_pool: Option<PathBuf>,
    /// 底表 taxonomy.json（默认 app_data_dir/taxonomy.json，缺失回退当前目录 taxonomy.json）
    #[arg(long = "base-taxonomy")]
    pub base_taxonomy: Option<PathBuf>,
    /// 生效分类树 taxonomy.effective.json（默认 app_data_dir/taxonomy.effective.json，缺失回退底表）
    #[arg(long = "effective-taxonomy")]
    pub effective_taxonomy: Option<PathBuf>,
    /// OpenAI 兼容端点（dashscope 默认官方兼容地址）
    #[arg(long = "base-url")]
    pub base_url: Option<String>,
    /// 从该文件读取 KEY=VALUE 环境变量（仅补缺失项；凭证不写代码/仓库）
    #[arg(long = "env-file")]
    pub env_file: Option<PathBuf>,
    /// API key 所在环境变量名
    #[arg(long = "api-key-env", default_value = "DASHSCOPE_API_KEY")]
    pub api_key_env: String,
    /// temperature（默认 0）
    #[arg(long)]
    pub temperature: Option<f64>,
    /// max_tokens（默认 400）
    #[arg(long = "max-tokens")]
    pub max_tokens: Option<u32>,
    /// 单次请求超时秒数（#11 classify 30s）
    #[arg(long = "timeout-secs")]
    pub timeout_secs: Option<u64>,
    /// #11 重试上限（默认 8）
    #[arg(long = "max-attempts")]
    pub max_attempts: Option<u32>,
    /// #11 退避基数秒（默认 30）
    #[arg(long = "retry-base-secs")]
    pub retry_base_secs: Option<u64>,
    /// #11 退避封顶秒（默认 1800）
    #[arg(long = "retry-max-secs")]
    pub retry_max_secs: Option<u64>,
    /// 显式代理（默认读 HTTPS_PROXY / HTTP_PROXY / ALL_PROXY）
    #[arg(long)]
    pub proxy: Option<String>,
    /// 强制直连（忽略代理环境变量）
    #[arg(long = "no-proxy")]
    pub no_proxy: bool,
    /// 不打印逐条进度
    #[arg(long)]
    pub quiet: bool,
}

impl EvalArgs {
    /// 是否所有 eval 参数均为默认值（用于拒绝与子命令混用）。
    fn is_empty(&self) -> bool {
        self.set.is_none()
            && self.provider.is_none()
            && self.model.is_none()
            && self.prompt_version.is_none()
            && self.limit.is_none()
            && self.report.is_none()
            && self.eval_set_version.is_none()
            && self.set_file.is_none()
            && self.fewshot.is_none()
            && self.tag_pool.is_none()
            && self.base_taxonomy.is_none()
            && self.effective_taxonomy.is_none()
            && self.base_url.is_none()
            && self.env_file.is_none()
            && self.api_key_env == "DASHSCOPE_API_KEY"
            && self.temperature.is_none()
            && self.max_tokens.is_none()
            && self.timeout_secs.is_none()
            && self.max_attempts.is_none()
            && self.retry_base_secs.is_none()
            && self.retry_max_secs.is_none()
            && self.proxy.is_none()
            && !self.no_proxy
            && !self.quiet
    }
}

#[derive(Args, Debug)]
pub struct CandidatesArgs {
    /// 输出目录（候选 JSONL + summary JSON）
    #[arg(long)]
    pub out: PathBuf,
    /// 数据库路径（默认 app_data_dir/aether.db）
    #[arg(long)]
    pub db: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct DedupeArgs {
    /// few-shot 配置 JSON
    #[arg(long)]
    pub fewshot: PathBuf,
    /// 主评测集 JSONL（visible + blind 同文件）
    #[arg(long)]
    pub set: PathBuf,
    /// 边界集 JSONL（重叠只告警）
    #[arg(long)]
    pub boundary: Option<PathBuf>,
    /// 输出机器可读 JSON
    #[arg(long)]
    pub json: bool,
}

type CliResult<T> = Result<T, String>;

pub fn run() -> i32 {
    let cli = Cli::parse();
    if cli.command.is_some() && !cli.eval.is_empty() {
        eprintln!("错误：eval 参数（--set/--model/--prompt-version 等）不能与 candidates/dedupe 子命令混用");
        return 2;
    }
    let data_dir = resolve_data_dir(cli.data_dir.as_deref());
    match cli.command {
        Some(Command::Candidates(args)) => run_candidates(args, &data_dir),
        Some(Command::Dedupe(args)) => run_dedupe(args),
        None => {
            let runtime = match tokio::runtime::Runtime::new() {
                Ok(runtime) => runtime,
                Err(err) => {
                    eprintln!("错误：初始化异步运行时失败：{err}");
                    return 2;
                }
            };
            match runtime.block_on(run_eval(cli.eval, &data_dir)) {
                Ok(()) => 0,
                Err(message) => {
                    eprintln!("错误：{message}");
                    2
                }
            }
        }
    }
}

fn run_candidates(args: CandidatesArgs, data_dir: &Path) -> i32 {
    let db_path = args.db.unwrap_or_else(|| data_dir.join("aether.db"));
    match generate_candidates(&db_path, &args.out) {
        Ok((summary, _)) => {
            println!("== aether-eval candidates 完成 ==");
            println!("数据库：{}", summary.db_path);
            println!("候选文件：{}", summary.out_file);
            println!("摘要文件：{}", summary.summary_file);
            println!(
                "纠偏行 {}：硬删跳过 {}，非分类维度排除 {}，缺条目 {}，按 entry_id 合并 {} 行，输出 {} 个候选",
                summary.total_correction_rows,
                summary.hard_deleted_rows_skipped,
                summary.excluded_non_classification_rows,
                summary.skipped_missing_entry_rows,
                summary.duplicate_rows_merged,
                summary.emitted_candidates
            );
            0
        }
        Err(err) => {
            eprintln!("错误：{err}");
            2
        }
    }
}

fn run_dedupe(args: DedupeArgs) -> i32 {
    match check_dedupe(&args.fewshot, &args.set, args.boundary.as_deref()) {
        Ok(report) => {
            if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                println!("{}", render_dedupe_text(&report));
            }
            report.exit_code()
        }
        Err(err) => {
            eprintln!("错误：{err}");
            2
        }
    }
}

async fn run_eval(args: EvalArgs, data_dir: &Path) -> CliResult<()> {
    if let Some(env_file) = &args.env_file {
        load_env_file(env_file)?;
    }
    let set_kind = SetKind::parse(
        args.set
            .as_deref()
            .ok_or("缺少 --set visible|blind|boundary")?,
    )?;
    let provider = args
        .provider
        .clone()
        .unwrap_or_else(|| "dashscope".to_string());
    let model = args
        .model
        .clone()
        .filter(|m| !m.trim().is_empty())
        .ok_or("缺少 --model")?;
    let prompt_version = args.prompt_version.ok_or("缺少 --prompt-version")?;

    let resolved_set = resolve_eval_set_path(
        data_dir,
        set_kind,
        args.eval_set_version.as_deref(),
        args.set_file.as_deref(),
    )?;
    let base_taxonomy_path = resolve_base_taxonomy(args.base_taxonomy.as_deref(), data_dir)?;
    let effective_path = args
        .effective_taxonomy
        .clone()
        .unwrap_or_else(|| data_dir.join("taxonomy.effective.json"));
    let tree = load_effective_taxonomy(&base_taxonomy_path, Some(&effective_path))
        .map_err(|e| e.to_string())?;

    let fewshot_path = args
        .fewshot
        .clone()
        .unwrap_or_else(|| data_dir.join("classifier").join("fewshot.json"));
    let few_shots = load_few_shots(&fewshot_path)
        .map_err(|e| format!("{e}（用 --fewshot 指定本地 few-shot 配置）"))?;
    let tag_pool_path = args
        .tag_pool
        .clone()
        .unwrap_or_else(|| data_dir.join("classifier").join("tag_pool.json"));
    let mut warnings: Vec<String> = Vec::new();
    let tag_pool = if tag_pool_path.is_file() {
        load_tag_pool(&tag_pool_path).map_err(|e| e.to_string())?
    } else {
        warnings.push(format!(
            "未找到特征标签候选池快照（{}），本次 prompt 不含候选池段",
            tag_pool_path.display()
        ));
        Vec::new()
    };

    let set = load_eval_set(&resolved_set.path).map_err(|e| e.to_string())?;
    if !set.parse_errors.is_empty() {
        warnings.push(format!(
            "评测集有 {} 行解析失败已跳过（见 set.parse_errors）",
            set.parse_errors.len()
        ));
    }
    let composition_stats = composition(&set.items);
    let comp_warnings = composition_warnings(&set.items, &tree);
    if set_kind.is_boundary() {
        warnings
            .push("边界集不参与门线，本次只统计输出分布与弃权/建议行为（#14 §3.5）".to_string());
    }
    if set_kind == SetKind::Blind {
        warnings
            .push("盲测集只应在候选版本定稿时运行一次；重复运行视为污染（#14 §4.3）".to_string());
    }
    let versions = check_versions(&set, &resolved_set.eval_set_version, &tree);
    let prepared = prepare_items(
        &set,
        &tree,
        set_kind.is_boundary(),
        match set_kind {
            SetKind::Visible => Some("visible"),
            SetKind::Blind => Some("blind"),
            SetKind::Boundary => None,
        },
        args.limit,
    );
    if prepared.remapped_count > 0 {
        warnings.push(format!(
            "{} 条金标引用的自建节点已按 merged_into 重映射（#18 §8.4）",
            prepared.remapped_count
        ));
    }
    if prepared.skipped_invalid > 0 {
        warnings.push(format!(
            "{} 条金标非法 / 缺失，已跳过且不计入分母",
            prepared.skipped_invalid
        ));
    }
    let valid_items: Vec<&PreparedItem> = prepared
        .items
        .iter()
        .filter(|item| item.invalid_reason.is_none())
        .collect();
    if valid_items.is_empty() {
        return Err("没有可评测条目（检查 split / --limit / 金标引用）".to_string());
    }
    if few_shots.len() != 8 {
        warnings.push(format!(
            "few-shot 条数为 {}（#19 §9 定稿为 8 条：6 正常 + 2 弃权）",
            few_shots.len()
        ));
    }
    if few_shots.len() > 8 {
        warnings.push("few-shot 超过 8 条会拉高 token 成本（#19 §10）".to_string());
    }

    let prompt = PromptConfig::new(prompt_version, tag_pool, few_shots);
    let api_key = std::env::var(&args.api_key_env).map_err(|_| {
        format!(
            "缺少 API key：请设置环境变量 {}（或用 --api-key-env / --env-file 指向本地凭证文件；凭证不写入代码与仓库）",
            args.api_key_env
        )
    })?;
    let provider_cfg = build_provider_config(&args, &provider, model, api_key)?;
    let retry = RetryPolicy {
        base_delay: Duration::from_secs(args.retry_base_secs.unwrap_or(30)),
        max_delay: Duration::from_secs(args.retry_max_secs.unwrap_or(1800)),
        max_attempts: args.max_attempts.unwrap_or(8),
        jitter_ratio: 0.20,
    };
    let retry_snapshot = retry.clone();
    let client = ChatClient::new(provider_cfg).map_err(|e| e.to_string())?;
    let engine =
        ClassificationEngine::try_new(client, prompt, tree, retry).map_err(|e| e.to_string())?;
    let params = ReportParams {
        provider: engine.provider_config().provider.clone(),
        model: engine.provider_config().model.clone(),
        prompt_version,
        template_hash: engine.prompt().template_hash(),
        fewshot_hash: engine.prompt().fewshot_hash(),
        fewshot_count: engine.prompt().few_shots.len(),
        tag_pool_hash: engine.prompt().tag_pool_hash(),
        tag_pool_count: engine.prompt().tag_pool.len(),
        temperature: engine.provider_config().temperature,
        max_tokens: engine.provider_config().max_tokens,
        base_url: engine.provider_config().base_url.clone(),
        request_timeout_ms: engine.provider_config().request_timeout.as_millis() as u64,
        retry_max_attempts: retry_snapshot.max_attempts,
        retry_base_secs: retry_snapshot.base_delay.as_secs(),
        retry_max_secs: retry_snapshot.max_delay.as_secs(),
    };

    let started_at = util::now_rfc3339();
    let run_started = std::time::Instant::now();
    let total = valid_items.len();
    let mut runs: Vec<RunItem> = Vec::with_capacity(total);
    for (index, prepared_item) in valid_items.iter().enumerate() {
        let item = &prepared_item.item;
        let entry = EntryForClassify::new(
            item.title.clone(),
            if item.url.trim().is_empty() {
                None
            } else {
                Some(item.url.clone())
            },
            item.media_type.clone(),
            if item.body_excerpt.trim().is_empty() {
                None
            } else {
                Some(item.body_excerpt.clone())
            },
        );
        let outcome = engine.classify(&entry).await;
        if !args.quiet {
            let status = match outcome.status {
                aether_classifier::OutcomeStatus::Ok => "ok",
                aether_classifier::OutcomeStatus::FormatInvalid => "format_invalid",
                aether_classifier::OutcomeStatus::Unavailable => "unavailable",
            };
            let predicted = outcome
                .classification
                .as_ref()
                .map(|c| format!("{} / {}", c.l1_id, c.l2_id))
                .unwrap_or_else(|| "-".to_string());
            println!(
                "[{}/{}] {} {} {} => {} ({}ms{})",
                index + 1,
                total,
                status,
                item.entry_id,
                util::truncate_chars(&item.title, 24),
                predicted,
                outcome.elapsed_ms,
                if outcome.repair_used {
                    ", repaired"
                } else {
                    ""
                }
            );
        }
        runs.push(RunItem {
            line: prepared_item.line,
            item: item.clone(),
            gold: prepared_item.gold.clone(),
            remapped_from: prepared_item.remapped_from.clone(),
            outcome,
        });
    }
    let duration_ms = run_started.elapsed().as_millis() as u64;
    let finished_at = util::now_rfc3339();
    let metrics = if set_kind.is_boundary() {
        None
    } else {
        Some(compute(&runs, engine.tree()))
    };
    let boundary = if set_kind.is_boundary() {
        Some(compute_boundary(&runs))
    } else {
        None
    };

    let taxonomy_effective_source = match engine.tree().effective_source {
        EffectiveSource::File => format!(
            "file（{}）",
            engine
                .tree()
                .effective_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default()
        ),
        EffectiveSource::BaseFallback => {
            "底表回退（未找到 taxonomy.effective.json，按 #18 §8.4 回退底表）".to_string()
        }
    };
    let report_versions = ReportVersions {
        eval_set_version: resolved_set.eval_set_version.clone(),
        eval_set_sha256: set.sha256.clone(),
        taxonomy_base_version: engine.tree().base_version.clone(),
        taxonomy_effective_version: engine.tree().effective_version.clone(),
        taxonomy_effective_source,
        taxonomy_effective_path: engine
            .tree()
            .effective_path
            .as_ref()
            .map(|p| p.display().to_string()),
        version_check: versions,
    };
    let report_stem = resolve_report_stem(args.report.as_deref(), data_dir);
    let reports_dir = report_stem
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| data_dir.join("eval").join("reports"));
    let index_path = reports_dir.join(BASELINE_INDEX_FILE);
    let combo = ComboInfo {
        provider: params.provider.clone(),
        model: params.model.clone(),
        prompt_version,
        eval_set_version: resolved_set.eval_set_version.clone(),
        taxonomy_base_version: report_versions.taxonomy_base_version.clone(),
        taxonomy_effective_version: report_versions.taxonomy_effective_version.clone(),
        split: set_kind.as_str().to_string(),
    };
    let baseline = if set_kind.is_boundary() || args.limit.is_some() {
        let mut comparison = crate::baseline::BaselineComparison::new_baseline(
            &crate::baseline::combo_key(&combo),
            report_versions.version_check.comparable,
        );
        if args.limit.is_some() {
            comparison.warnings.push(
                "受限运行（--limit）只用于调试：不登记基线、不与基线比较（#14 §4.4）".to_string(),
            );
        }
        comparison
    } else {
        let rates = metrics.as_ref().map(|m| m.rates()).unwrap_or_default();
        compare_and_register(
            &index_path,
            &combo,
            &set.sha256,
            &report_stem.with_extension("json"),
            &rates,
            &finished_at,
            report_versions.version_check.comparable,
        )
        .map_err(|e| e.to_string())?
    };
    if set_kind == SetKind::Blind {
        let previous = count_previous_runs(&reports_dir, &baseline.combo_key, "blind");
        if previous > 0 {
            warnings.push(format!(
                "同一候选版本的盲测集已运行 {} 次，本次为第 {} 次：多跑视为污染并记录（#14 §4.3）",
                previous,
                previous + 1
            ));
        }
    }

    let gold_l2: HashSet<&str> = runs
        .iter()
        .filter_map(|run| run.gold.as_ref())
        .map(|gold| gold.l2_id.as_str())
        .collect();
    let uncovered: Vec<UncoveredNode> = engine
        .tree()
        .self_built_l2_ids()
        .into_iter()
        .filter(|id| !gold_l2.contains(id))
        .map(|id| UncoveredNode {
            id: id.to_string(),
            name: engine.tree().l2_name(id).unwrap_or("").to_string(),
        })
        .collect();

    let feature_review = metrics.as_ref().and_then(|m| {
        if m.classified == 0 {
            return None;
        }
        let sampled = sample_feature_review(&runs).len();
        Some(FeatureReviewInfo {
            sampled,
            file: report_stem
                .with_extension("feature-review.jsonl")
                .display()
                .to_string(),
        })
    });
    let set_info = SetInfo {
        kind: set_kind.as_str().to_string(),
        file: resolved_set.path.display().to_string(),
        total_file_items: set.items.len(),
        evaluated: runs.len(),
        skipped_invalid: prepared.skipped_invalid,
        limit: args.limit,
        parse_errors: set.parse_errors.clone(),
        composition: composition_stats,
        composition_warnings: comp_warnings.clone(),
    };
    let run_info = RunInfo {
        started_at,
        finished_at: finished_at.clone(),
        duration_ms,
    };
    let ctx = ReportsContext {
        params: &params,
        versions: &report_versions,
        set: &set_info,
        run: &run_info,
        metrics: metrics.as_ref(),
        boundary: boundary.as_ref(),
        baseline: &baseline,
        uncovered_l2: &uncovered,
        warnings: warnings.clone(),
        feature_review,
        items: &runs,
    };
    let paths = write_reports(report_paths(&report_stem), &ctx).map_err(|e| e.to_string())?;

    println!();
    println!("== aether-eval 完成（{}） ==", set_kind.as_str());
    println!("报告：{}", paths.md.display());
    println!("JSON：{}", paths.json.display());
    println!("逐条结果：{}", paths.items_jsonl.display());
    if let Some(review) = &paths.feature_review {
        println!("特征标签抽样：{}", review.display());
    }
    if let Some(metrics) = &metrics {
        for (label, value) in rates_table(&metrics.rates()) {
            println!("{label}：{value}");
        }
        let checks = gate_checks(&metrics.rates());
        let overall = match gate_overall(&checks) {
            Some(true) => "PASS",
            Some(false) => "FAIL",
            None => "—",
        };
        println!("发布门线：{overall}");
        if baseline.is_baseline {
            println!("基线：本次为新基线（组合 {}）", baseline.combo_key);
        } else {
            println!(
                "基线：与 {} 比较（{}）",
                baseline.baseline_report.clone().unwrap_or_default(),
                if baseline.warnings.is_empty() {
                    "无回归告警".to_string()
                } else {
                    baseline.warnings.join("；")
                }
            );
        }
        if !comp_warnings.is_empty() {
            println!("评测集分层警告：{} 条（见报告）", comp_warnings.len());
        }
    }
    if let Some(boundary) = &boundary {
        println!(
            "边界集：弃权 {}/{}，suggestion {} 条，失败 {} 条",
            boundary.abstained, boundary.classified, boundary.with_suggestion, boundary.failed
        );
    }
    if !warnings.is_empty() {
        println!();
        for warning in &warnings {
            println!("⚠ {warning}");
        }
    }
    Ok(())
}

fn build_provider_config(
    args: &EvalArgs,
    provider: &str,
    model: String,
    api_key: String,
) -> CliResult<ProviderConfig> {
    let is_dashscope = provider.eq_ignore_ascii_case("dashscope")
        || provider.eq_ignore_ascii_case("bailian")
        || provider.eq_ignore_ascii_case("dashscope-compatible");
    let base_url = match &args.base_url {
        Some(url) => url.trim().trim_end_matches('/').to_string(),
        None if is_dashscope => "https://dashscope.aliyuncs.com/compatible-mode/v1".to_string(),
        None => {
            return Err(format!(
                "provider {provider} 不是 dashscope，需显式 --base-url（OpenAI 兼容端点）"
            ))
        }
    };
    let proxy = if args.no_proxy {
        None
    } else if let Some(proxy) = &args.proxy {
        Some(proxy.clone())
    } else {
        [
            "AETHER_HTTPS_PROXY",
            "HTTPS_PROXY",
            "HTTP_PROXY",
            "ALL_PROXY",
        ]
        .iter()
        .find_map(|key| std::env::var(key).ok().filter(|v| !v.trim().is_empty()))
    };
    let mut extra_body = Map::new();
    if is_dashscope {
        extra_body.insert("enable_thinking".to_string(), serde_json::json!(false));
    }
    Ok(ProviderConfig {
        provider: provider.to_string(),
        base_url,
        model,
        api_key,
        temperature: args.temperature.unwrap_or(0.0),
        max_tokens: args.max_tokens.unwrap_or(400),
        request_timeout: Duration::from_secs(args.timeout_secs.unwrap_or(30)),
        proxy,
        extra_body,
    })
}

fn resolve_base_taxonomy(explicit: Option<&Path>, data_dir: &Path) -> CliResult<PathBuf> {
    if let Some(path) = explicit {
        if !path.is_file() {
            return Err(format!("底表 taxonomy 不存在：{}", path.display()));
        }
        return Ok(path.to_path_buf());
    }
    let in_data = data_dir.join("taxonomy.json");
    if in_data.is_file() {
        return Ok(in_data);
    }
    let cwd = PathBuf::from("taxonomy.json");
    if cwd.is_file() {
        return Ok(cwd);
    }
    Err(format!(
        "找不到底表 taxonomy.json（试过 {} 与当前目录），用 --base-taxonomy 指定",
        in_data.display()
    ))
}

fn resolve_report_stem(report: Option<&Path>, data_dir: &Path) -> PathBuf {
    if let Some(path) = report {
        let has_report_ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.eq_ignore_ascii_case("json") || e.eq_ignore_ascii_case("md"))
            .unwrap_or(false);
        return if has_report_ext {
            path.with_extension("")
        } else {
            path.to_path_buf()
        };
    }
    data_dir
        .join("eval")
        .join("reports")
        .join(util::file_timestamp())
}

fn load_env_file(path: &Path) -> CliResult<()> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 --env-file 失败（{}）：{e}", path.display()))?;
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(format!(
                "--env-file 第 {} 行不是 KEY=VALUE：{}",
                index + 1,
                util::truncate_chars(line, 60)
            ));
        };
        let key = key.trim();
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if key.is_empty() {
            continue;
        }
        if std::env::var_os(key).is_none() {
            std::env::set_var(key, value);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_stem_strips_json_md_suffix() {
        let stem = resolve_report_stem(Some(Path::new("x/y.json")), Path::new("ignored"));
        assert_eq!(stem, PathBuf::from("x/y"));
        let stem = resolve_report_stem(Some(Path::new("x/y")), Path::new("ignored"));
        assert_eq!(stem, PathBuf::from("x/y"));
    }

    #[test]
    fn env_file_sets_missing_vars_only() {
        let path = std::env::temp_dir().join(format!(
            "aether-env-test-{}-{}.env",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::write(&path, "# comment\nAETHER_TEST_KEY_XYZ=abc\n").unwrap();
        load_env_file(&path).unwrap();
        assert_eq!(std::env::var("AETHER_TEST_KEY_XYZ").unwrap(), "abc");
        let _ = std::fs::remove_file(&path);
    }
}
