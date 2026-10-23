//! 本地录入（#28）：把搜索框 / 快速录入弹窗的纯文本变成入库计划。
//!
//! URL 走 fetch stage（抓取正文后再分类），纯文本直入 classify stage（不浪费一次
//! 抓取）。两条路径都在同一个事务里写 entries + entry_sources + pipeline_jobs，
//! 由 storage 层保证；去重双轨（来源外部 id → canonical URL 哈希）也在那里。

use serde::Serialize;

use crate::url;

/// body_md 存储上限（字符）：覆盖分类上下文（#10 §12）。
pub const BODY_STORE_CHARS: usize = 1500;

/// excerpt 上限（字符）：进 FTS 的正文（#10 §12 / ADR-0002）。
pub const FTS_EXCERPT_CHARS: usize = 500;

/// 纯文本条目的标题上限（字符）。
pub const TITLE_MAX_CHARS: usize = 120;

/// fail-safe 分类：分类引擎落位前，条目暂挂「00 未分类 / 待人工归档」。
pub const FAIL_SAFE_L1_ID: &str = "l1_inbox";
pub const FAIL_SAFE_L2_ID: &str = "l2_manual_review";

/// 条目固有媒体类型（#9：不再是 L3 标签）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Link,
    Video,
    Article,
}

impl MediaType {
    pub fn as_str(self) -> &'static str {
        match self {
            MediaType::Link => "link",
            MediaType::Video => "video",
            MediaType::Article => "article",
        }
    }
}

/// 管线起点 stage（#11：纯文本直入 classify，URL 先进 fetch）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStage {
    Fetch,
    Classify,
}

impl PipelineStage {
    pub fn as_str(self) -> &'static str {
        match self {
            PipelineStage::Fetch => "fetch",
            PipelineStage::Classify => "classify",
        }
    }
}

/// 一次本地录入的完整入库计划；storage 负责在一个事务里落三张表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestPlan {
    pub uid: String,
    pub title: String,
    pub url: Option<String>,
    pub canonical_url: Option<String>,
    pub url_hash: Option<[u8; url::URL_HASH_BYTES]>,
    pub media_type: MediaType,
    pub body_md: Option<String>,
    pub excerpt: Option<String>,
    pub l1_id: String,
    pub l2_id: String,
    pub source_type: String,
    /// 本次投递的幂等键：手动录入使用新生成的 ULID（URL 去重由 canonical 轨道兜底）。
    pub source_ref: String,
    pub stage: PipelineStage,
}

/// 入库结果（IPC 返回给前端，用于提示与收件箱刷新）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestOutcome {
    /// 新建或命中的条目 id。
    pub entry_id: i64,
    /// true = 双轨去重命中，只更新了最近出现时间。
    pub duplicate: bool,
    /// 仅新建条目返回：本次入队的 stage（重复投递不产生新 job）。
    pub stage: Option<PipelineStage>,
    pub pipeline_status: String,
    pub title: String,
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IngestError {
    #[error("录入内容为空")]
    EmptyInput,
}

/// 把用户输入变成入库计划：能归一化成 http(s) URL 的走抓取，其余按纯文本。
pub fn plan_manual(text: &str, now_ms: i64) -> Result<IngestPlan, IngestError> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(IngestError::EmptyInput);
    }

    let source_ref = crate::ids::new_ulid(now_ms);
    let uid = crate::ids::new_ulid(now_ms);

    if let Some(canonical) = url::normalize(trimmed) {
        // 视频来源的元数据抓取在 #33 接入；录入时先按普通链接入 fetch stage，
        // 抓取成功后由 worker 回写 media_type（#24 §4 L0）。
        return Ok(IngestPlan {
            uid,
            title: trimmed.to_string(),
            url: Some(trimmed.to_string()),
            canonical_url: Some(canonical.canonical),
            url_hash: Some(canonical.hash),
            media_type: MediaType::Link,
            body_md: None,
            excerpt: None,
            l1_id: FAIL_SAFE_L1_ID.to_string(),
            l2_id: FAIL_SAFE_L2_ID.to_string(),
            source_type: "manual".to_string(),
            source_ref,
            stage: PipelineStage::Fetch,
        });
    }

    Ok(IngestPlan {
        uid,
        title: derive_title(trimmed),
        url: None,
        canonical_url: None,
        url_hash: None,
        media_type: MediaType::Article,
        body_md: Some(truncate_chars(trimmed, BODY_STORE_CHARS)),
        excerpt: Some(truncate_chars(trimmed, FTS_EXCERPT_CHARS)),
        l1_id: FAIL_SAFE_L1_ID.to_string(),
        l2_id: FAIL_SAFE_L2_ID.to_string(),
        source_type: "manual".to_string(),
        source_ref,
        stage: PipelineStage::Classify,
    })
}

/// 标题取首行（折叠内部空白），无首行时取整段；超过上限截断。
fn derive_title(text: &str) -> String {
    let first_line = text
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(text);
    let collapsed = first_line.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&collapsed, TITLE_MAX_CHARS)
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::is_ulid;

    const NOW: i64 = 1_761_000_000_000;

    #[test]
    fn empty_input_is_rejected() {
        assert_eq!(plan_manual("", NOW), Err(IngestError::EmptyInput));
        assert_eq!(plan_manual("  \n\t ", NOW), Err(IngestError::EmptyInput));
    }

    #[test]
    fn url_input_goes_to_fetch_stage_with_canonical_hash() {
        let plan = plan_manual("  https://Example.com/a?utm_source=x&b=2&a=1#frag  ", NOW).unwrap();
        assert_eq!(plan.stage, PipelineStage::Fetch);
        assert_eq!(plan.media_type, MediaType::Link);
        assert_eq!(
            plan.title,
            "https://Example.com/a?utm_source=x&b=2&a=1#frag"
        );
        assert_eq!(
            plan.url.as_deref(),
            Some("https://Example.com/a?utm_source=x&b=2&a=1#frag")
        );
        assert_eq!(
            plan.canonical_url.as_deref(),
            Some("https://example.com/a?a=1&b=2")
        );
        assert!(plan.url_hash.is_some());
        assert_eq!(plan.body_md, None);
        assert_eq!(plan.excerpt, None);
        assert_eq!(plan.l1_id, FAIL_SAFE_L1_ID);
        assert_eq!(plan.l2_id, FAIL_SAFE_L2_ID);
        assert_eq!(plan.source_type, "manual");
        assert!(is_ulid(&plan.uid));
        assert!(is_ulid(&plan.source_ref));
        assert_ne!(plan.uid, plan.source_ref);
    }

    #[test]
    fn plain_text_goes_straight_to_classify_stage() {
        let plan = plan_manual("  第一行标题\n第二行正文  ", NOW).unwrap();
        assert_eq!(plan.stage, PipelineStage::Classify);
        assert_eq!(plan.media_type, MediaType::Article);
        assert_eq!(plan.title, "第一行标题");
        assert_eq!(plan.url, None);
        assert_eq!(plan.canonical_url, None);
        assert_eq!(plan.url_hash, None);
        assert_eq!(plan.body_md.as_deref(), Some("第一行标题\n第二行正文"));
        assert_eq!(plan.excerpt.as_deref(), Some("第一行标题\n第二行正文"));
    }

    #[test]
    fn no_scheme_is_plain_text_not_a_url() {
        let plan = plan_manual("example.com/page", NOW).unwrap();
        assert_eq!(plan.stage, PipelineStage::Classify);
        assert_eq!(plan.url, None);
    }

    #[test]
    fn body_and_excerpt_are_capped_by_character_count() {
        let body = "中".repeat(BODY_STORE_CHARS + 200);
        let plan = plan_manual(&body, NOW).unwrap();
        assert_eq!(plan.body_md.as_deref().unwrap().chars().count(), 1500);
        assert_eq!(plan.excerpt.as_deref().unwrap().chars().count(), 500);

        let long_title = "a".repeat(TITLE_MAX_CHARS + 10);
        let plan = plan_manual(&long_title, NOW).unwrap();
        assert_eq!(plan.title.chars().count(), TITLE_MAX_CHARS);
    }
}
