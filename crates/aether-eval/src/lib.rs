//! aether-eval：AetherHUD 本地分类质量评测工具（#21）。
//!
//! 子命令：
//! - eval（默认）：读金标集、调用分类器、计算 #14 §2.3 指标、输出 Markdown + JSON 报告
//! - candidates：corrections JOIN entries → 纠偏候选 JSONL
//! - dedupe：few-shot 池与评测集查重
//!
//! 报告落 app_data_dir/eval/reports/，评测读数不写生产库。

pub mod appdata;
pub mod baseline;
pub mod candidates;
pub mod cli;
pub mod dedupe;
pub mod eval_set;
pub mod metrics;
pub mod report;
pub mod util;
