//! AetherHUD 分类路径（#19 / #14 施工对象）。
//!
//! 本 crate 是分类器与评测工具的唯一实现：
//! - 分类树视图：文档解析、校验与 prompt 注入（taxonomy；文件读取与合并见 aether-core）
//! - prompt 组装：定稿 V-C4（prompt）
//! - 输出校验与归一化（schema）
//! - OpenAI 兼容 provider（provider）与 #11 重试策略（retry）
//! - 端到端分类引擎（engine）
//!
//! 未来迁入 `src-tauri/src/classifier/` 时，模块可原样搬移；`aether-eval` 的 bin
//! 对应 `src-tauri/src/bin/aether-eval.rs`（#21 允许先独立 crate）。

pub mod config;
pub mod engine;
pub mod entry;
pub mod prompt;
pub mod provider;
pub mod retry;
pub mod schema;
pub mod taxonomy;

pub use engine::{
    CallPurpose, CallRecord, ClassificationEngine, ClassificationOutcome, OutcomeStatus,
};
pub use entry::{EntryForClassify, VideoMeta};
pub use prompt::{FewShotExample, Message, PromptConfig};
pub use provider::{ChatClient, ChatResponse, ProviderConfig, ProviderError, Usage};
pub use retry::{ErrorClass, RetryPolicy};
pub use schema::{
    validate_classification, Classification, NewL2, Suggestion, ValidationCode, ValidationError,
    ValidationIssue, ValidationOutcome,
};
pub use taxonomy::{
    base_version_of, base_view_from_json, effective_view_from_json, EffectiveSource, TaxonomyView,
    L1_INBOX, L2_MANUAL_REVIEW,
};
