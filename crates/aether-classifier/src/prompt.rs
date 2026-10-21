//! 分类 prompt 组装（#19 §3 定稿 V-C4 结构）。
//!
//! 生产分类路径与 aether-eval 共用 build_classification_messages；
//! system 模板骨架入库、few-shot 实例与候选池快照存本地配置（#14 §3.4 / §6.4）。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::entry::EntryForClassify;
use crate::schema::{validate_classification, Classification, ValidationError, ValidationIssue};
use crate::taxonomy::TaxonomyView;

pub const FEWSHOT_INTRO: &str = "【示例】下面是已核对过的分类示例，注意判定口径与 JSON 形态。";
pub const FEWSHOT_ACK: &str = "好的，请给待分类条目。";

const SEG_ROLE: &str = "你是 AetherHUD 的内容分类器。用户会给你一条自己收藏的内容（标题、链接、媒体类型、正文摘录），你要把它归入固定分类树中的一个 L2，并给出筛选标签。";
const SEG_TREE: &str = "【分类树】（只能从这些节点中选，禁止发明分类）\n{{TAXONOMY_TREE}}";
const SEG_JUDGE: &str = "【判定方法】\n1. 先判断 L1（内容与表达 / 效率与工作流 / 技术与工程 / 商业与认知），再在该 L1 下选最合适的 L2。\n2. 以「用户以后会去哪里找它」为准；跨类时按主要用途归类。\n3. 媒体类型不是分类依据：视频、文章只是载体，讲编程的视频归 l2_coding，不因是视频就归「视频与音频」。\n4. 标题很短、只有链接或信息不全时，从正文摘录和链接域名判断意图。";
const SEG_TAGS: &str = "【标签】\n- format_tags（形态标签）：从固定 5 个 id 中选 1~2 个；多数内容 1 个就够，只有确实同时呈现两种形态（如「工具 + 教程」）才给 2 个。\n  tag_tool=工具、tag_tutorial=教程、tag_prompt=提示词、tag_workflow=工作流、tag_asset=素材/资源\n- feature_tags（特征标签）：最多 3 个，宁少不多；只取原文里最关键、用户真会用来搜索的专有名词，每个 1~20 字；不要泛词（AI、工具、大模型）。";
const SEG_POOL: &str =
    "【特征标签候选池】（优先复用，写法保持一致；确有更准确的新词才新增）\n{{TAG_POOL}}";
const SEG_ABSTAIN: &str = "【何时弃权】\n只有两种情况允许弃权（输出 l1_inbox + l2_manual_review）：\n1. 几乎没有可判断的信息：只有一句噪声、一个域名或裸链接，看不出主题；\n2. 主题清楚，但确实不属于分类树里的任何一个正常 L2（共 {{NORMAL_L2_COUNT}} 个）。\n其余情况（不确定、跨类、标题短但有正文、在两个 L2 之间犹豫）都要选最可能的一个，不要弃权；弃权率目标 ≤5%。";
const SEG_UNCOVERED: &str = "已知不在覆盖范围的主题例子：通识学习（教材/课程/科普）、生活健康（健身/养生/医疗）、时事新闻（纯资讯）、纯娱乐（段子/追星）。";
const SEG_SUGGEST: &str = "【弃权时给建议】\n弃权时额外输出 suggestion，帮助人快速归置；非弃权时 suggestion 固定为 null。\n{\"nearest_l2_id\":\"最接近的正常 L2 id，确实无关联时为 null\",\"new_l2\":{\"parent_l1_id\":\"4 个正常 L1 之一\",\"name\":\"2~10 字的分类名\"} 或 null}\n- new_l2 只在确有类别缺口时给；名字要短、具体、可复用（如「短视频脚本」「交易与投资」）。\n- 建议只是线索、不参与评分；不要为了能填建议而弃权。";
const SEG_CONTRACT: &str = "【输出】只输出一个 JSON 对象，不要解释、不要 Markdown 代码块。\n{\"l1_id\":\"l1_xxx\",\"l2_id\":\"l2_xxx\",\"format_tags\":[\"tag_xxx\"],\"feature_tags\":[\"xxx\"],\"suggestion\":null}";
const SEG_SELF_CHECK: &str = "【输出前自检】\n- l1_id / l2_id 都在分类树中，且 l2_id 属于该 l1_id；\n- format_tags 为 1~2 个合法 id；feature_tags 不超过 3 个、每个 1~20 字；\n- 弃权时 suggestion 非 null 且字段合法；非弃权时 suggestion 为 null。";

const SYSTEM_SEGMENTS: &[&str] = &[
    SEG_ROLE,
    SEG_TREE,
    SEG_JUDGE,
    SEG_TAGS,
    SEG_POOL,
    SEG_ABSTAIN,
    SEG_UNCOVERED,
    SEG_SUGGEST,
    SEG_CONTRACT,
    SEG_SELF_CHECK,
];

/// 模板骨架（含占位符）——template_hash 的计算对象，变化即 prompt 版本应 +1。
pub fn template_source() -> String {
    SYSTEM_SEGMENTS.join("\n\n")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

impl Message {
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: "system".to_string(),
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: "user".to_string(),
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: "assistant".to_string(),
            content: content.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FewShotExample {
    #[serde(default)]
    pub entry_id: Option<String>,
    pub input: EntryForClassify,
    pub output: Classification,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PromptConfig {
    pub prompt_version: u32,
    pub tag_pool: Vec<String>,
    pub few_shots: Vec<FewShotExample>,
}

#[derive(Debug, thiserror::Error)]
pub enum PromptConfigError {
    #[error("few-shot 第 {index} 条输出不合法：{source}")]
    InvalidFewShot {
        index: usize,
        #[source]
        source: ValidationError,
    },
    #[error("few-shot 第 {index} 条 input.title 为空")]
    EmptyFewShotTitle { index: usize },
}

impl PromptConfig {
    pub fn new(prompt_version: u32, tag_pool: Vec<String>, few_shots: Vec<FewShotExample>) -> Self {
        Self {
            prompt_version,
            tag_pool,
            few_shots,
        }
    }

    pub fn template_hash(&self) -> String {
        sha256_hex8(&template_source())
    }

    pub fn fewshot_hash(&self) -> String {
        let canonical = serde_json::to_string(&self.few_shots).unwrap_or_default();
        sha256_hex8(&canonical)
    }

    pub fn tag_pool_hash(&self) -> String {
        sha256_hex8(&self.tag_pool.join("\n"))
    }

    /// 校验本地 few-shot 配置：输出必须能通过同一校验器（否则 prompt 里会教错格式）。
    pub fn validate(&self, tree: &TaxonomyView) -> Result<(), PromptConfigError> {
        for (index, example) in self.few_shots.iter().enumerate() {
            if example.input.title.trim().is_empty() {
                return Err(PromptConfigError::EmptyFewShotTitle { index });
            }
            let raw = serde_json::to_string(&example.output).unwrap_or_default();
            if let Err(source) = validate_classification(&raw, tree) {
                return Err(PromptConfigError::InvalidFewShot { index, source });
            }
        }
        Ok(())
    }

    /// #19 §12 约定的共用组装函数（评测与生产都走这里）。
    pub fn build_messages(&self, tree: &TaxonomyView, entry: &EntryForClassify) -> Vec<Message> {
        build_classification_messages(tree, &self.tag_pool, &self.few_shots, entry)
    }
}

pub fn build_system_prompt(tree: &TaxonomyView, tag_pool: &[String]) -> String {
    let tree_text = tree.render_tree();
    let count = tree.normal_l2_count().to_string();
    let pool_block: Option<String> = if tag_pool.is_empty() {
        None
    } else {
        Some(
            SEG_POOL
                .replace("{{TAG_POOL}}", &tag_pool.join("、"))
                .to_string(),
        )
    };
    let mut parts: Vec<String> = Vec::with_capacity(SYSTEM_SEGMENTS.len());
    for seg in SYSTEM_SEGMENTS {
        if *seg == SEG_POOL {
            if let Some(block) = &pool_block {
                parts.push(block.clone());
            }
            continue;
        }
        let rendered = seg
            .replace("{{TAXONOMY_TREE}}", &tree_text)
            .replace("{{NORMAL_L2_COUNT}}", &count);
        parts.push(rendered);
    }
    parts.join("\n\n")
}

/// 规格命名的唯一组装入口（#19 §12）。
pub fn build_classification_messages(
    tree: &TaxonomyView,
    tag_pool: &[String],
    few_shots: &[FewShotExample],
    entry: &EntryForClassify,
) -> Vec<Message> {
    let mut messages = Vec::with_capacity(2 + few_shots.len() * 2 + 1);
    messages.push(Message::system(build_system_prompt(tree, tag_pool)));
    if !few_shots.is_empty() {
        messages.push(Message::user(FEWSHOT_INTRO));
        messages.push(Message::assistant(FEWSHOT_ACK));
        for example in few_shots {
            messages.push(Message::user(user_content(&example.input)));
            let out = serde_json::to_string(&example.output).unwrap_or_else(|_| "{}".to_string());
            messages.push(Message::assistant(out));
        }
    }
    messages.push(Message::user(user_content(entry)));
    messages
}

pub fn repair_messages(
    messages: &[Message],
    raw_output: &str,
    issues: &[ValidationIssue],
) -> Vec<Message> {
    let mut out = messages.to_vec();
    out.push(Message::assistant(raw_output));
    let mut lines = vec!["上面的输出未通过校验：".to_string()];
    for issue in issues {
        lines.push(format!("- {:?}: {}", issue.code, issue.detail));
    }
    lines.push("请只输出修正后的 JSON 对象，不要解释。".to_string());
    out.push(Message::user(lines.join("\n")));
    out
}

/// 【待分类条目】字段块（#19 §3.3 / §4）。
pub fn user_content(entry: &EntryForClassify) -> String {
    let title = normalize_title(&entry.title, entry.body_excerpt.as_deref());
    let mut lines = vec!["【待分类条目】".to_string(), format!("标题：{title}")];
    if let Some(url) = entry.url.as_deref().map(simplify_url) {
        if !url.is_empty() {
            lines.push(format!("链接：{url}"));
        }
    }
    lines.push(format!("媒体类型：{}", media_label(&entry.media_type)));
    if let Some(meta) = &entry.video_meta {
        let mut parts: Vec<String> = Vec::new();
        if let Some(author) = meta.author.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(format!("作者：{}", author.trim()));
        }
        if let Some(platform) = meta.platform.as_deref().filter(|s| !s.trim().is_empty()) {
            parts.push(format!("平台：{}", platform.trim()));
        }
        if let Some(desc) = meta.description_excerpt.as_deref() {
            let desc = collapse_whitespace(desc);
            if !desc.is_empty() {
                parts.push(format!("简介：{}", truncate_chars(&desc, 300)));
            }
        }
        if !parts.is_empty() {
            lines.push(format!("视频信息：{}", parts.join(" / ")));
        }
    }
    lines.push("正文摘录：".to_string());
    lines.push(body_excerpt_text(entry.body_excerpt.as_deref()));
    lines.join("\n")
}

fn media_label(media_type: &str) -> &str {
    match media_type {
        "link" => "网页链接",
        "video" => "视频",
        "article" => "文章",
        "text" => "纯文本",
        other => other,
    }
}

fn simplify_url(url: &str) -> String {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let lower = trimmed.to_ascii_lowercase();
    let scheme_len = if lower.starts_with("https://") {
        8
    } else if lower.starts_with("http://") {
        7
    } else {
        0
    };
    let mut rest = trimmed[scheme_len..].to_string();
    if let Some(stripped) = rest.strip_prefix("www.") {
        rest = stripped.to_string();
    }
    if let Some(pos) = rest.find(['?', '#']) {
        rest.truncate(pos);
    }
    truncate_chars(rest.trim_end_matches('/'), 80)
}

fn normalize_title(title: &str, body: Option<&str>) -> String {
    let normalized = collapse_whitespace(title);
    if !normalized.is_empty() {
        return truncate_chars(&normalized, 200);
    }
    let from_body = body
        .unwrap_or("")
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(collapse_whitespace)
        .unwrap_or_default();
    if from_body.is_empty() {
        "（无标题）".to_string()
    } else {
        truncate_chars(&from_body, 200)
    }
}

fn body_excerpt_text(body: Option<&str>) -> String {
    let Some(body) = body else {
        return "（无正文）".to_string();
    };
    let normalized = normalize_body(body);
    if normalized.is_empty() {
        return "（无正文）".to_string();
    }
    truncate_chars(&normalized, 1500)
}

fn normalize_body(body: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut blank_run = 0usize;
    for raw in body.lines() {
        let line = raw.trim_end().trim_start();
        if line.is_empty() {
            blank_run += 1;
            if blank_run == 1 && !lines.is_empty() {
                lines.push(String::new());
            }
        } else {
            blank_run = 0;
            lines.push(line.to_string());
        }
    }
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    lines.join("\n")
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

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

pub fn sha256_hex8(s: &str) -> String {
    let digest = Sha256::digest(s.as_bytes());
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::EntryForClassify;
    use crate::schema::Classification;
    use crate::taxonomy::load_base_taxonomy;

    const BASE: &str = r#"{
      "version": "1.1.0",
      "global_l3_tags": {"format_tags": [
        {"id":"tag_tool","name":"工具"},{"id":"tag_tutorial","name":"教程"},
        {"id":"tag_prompt","name":"提示词"},{"id":"tag_workflow","name":"工作流"},
        {"id":"tag_asset","name":"素材/资源"}
      ]},
      "taxonomy": [
        {"id":"l1_inbox","name":"00 未分类","children":[{"id":"l2_manual_review","name":"待人工归档","l3_preset_tags":[]}]},
        {"id":"l1_tech","name":"03 技术与工程","children":[
          {"id":"l2_coding","name":"编程与低代码","l3_preset_tags":["Cursor"]},
          {"id":"l2_prompt_agent","name":"提示工程与智能体","l3_preset_tags":["Coze"]}
        ]},
        {"id":"l1_biz","name":"04 商业与认知","children":[
          {"id":"l2_insight","name":"行业观察与研报","l3_preset_tags":[]}
        ]}
      ]
    }"#;

    fn tree() -> TaxonomyView {
        let path = std::env::temp_dir().join(format!(
            "aether-prompt-test-{}-{}.json",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::write(&path, BASE).unwrap();
        load_base_taxonomy(&path).unwrap()
    }

    fn fewshot(entry_id: &str, title: &str, l1: &str, l2: &str) -> FewShotExample {
        FewShotExample {
            entry_id: Some(entry_id.to_string()),
            input: EntryForClassify::new(title, None, "link", Some("正文".to_string())),
            output: Classification {
                l1_id: l1.to_string(),
                l2_id: l2.to_string(),
                format_tags: vec!["tag_tool".to_string()],
                feature_tags: vec![],
                suggestion: None,
            },
            note: None,
        }
    }

    fn sample() -> EntryForClassify {
        EntryForClassify::new(
            "标题  有   空白",
            Some("https://Example.com/path?utm=1#frag".to_string()),
            "video",
            Some("第一行\n\n\n第二行".to_string()),
        )
    }

    #[test]
    fn system_prompt_injects_tree_pool_and_contract() {
        let view = tree();
        let prompt = build_system_prompt(&view, &["Cursor".to_string(), "Coze".to_string()]);
        assert!(prompt.contains("l1_inbox 00 未分类（仅用于弃权）"));
        assert!(prompt.contains("共 3 个"));
        assert!(prompt.contains("特征标签候选池"));
        assert!(prompt.contains("Cursor、Coze"));
        assert!(prompt.contains("\"suggestion\":null"));
        assert!(prompt.find("l1_tech").unwrap() < prompt.find("l1_inbox").unwrap());
        // 空候选池时整段省略
        let empty = build_system_prompt(&view, &[]);
        assert!(!empty.contains("特征标签候选池"));
    }

    #[test]
    fn message_shape_matches_spec() {
        let view = tree();
        let few_shots = vec![
            fewshot("01A", "示例一", "l1_tech", "l2_coding"),
            fewshot("01B", "示例二", "l1_biz", "l2_insight"),
        ];
        let messages = build_classification_messages(&view, &[], &few_shots, &sample());
        assert_eq!(messages.len(), 3 + few_shots.len() * 2 + 1);
        assert_eq!(messages[0].role, "system");
        assert_eq!(messages[1].role, "user");
        assert_eq!(messages[1].content, FEWSHOT_INTRO);
        assert_eq!(messages[2].content, FEWSHOT_ACK);
        assert_eq!(messages.last().unwrap().role, "user");
        assert!(messages.last().unwrap().content.contains("【待分类条目】"));
        assert_eq!(messages[4].role, "assistant");
        assert!(messages[4].content.contains("\"suggestion\":null"));
    }

    #[test]
    fn user_content_simplifies_url_and_collapses_body() {
        let content = user_content(&sample());
        assert!(content.contains("标题：标题 有 空白"));
        assert!(content.contains("链接：Example.com/path"));
        assert!(!content.contains("utm"));
        assert!(content.contains("媒体类型：视频"));
        assert!(content.contains("第一行\n\n第二行"));
    }

    #[test]
    fn body_is_truncated_to_1500_chars() {
        let long = "字".repeat(1600);
        let entry = EntryForClassify::new("t", None, "link", Some(long));
        let content = user_content(&entry);
        let body = content.split("正文摘录：\n").nth(1).unwrap();
        assert_eq!(body.chars().count(), 1500);
    }

    #[test]
    fn prompt_config_validates_fewshots() {
        let view = tree();
        let good = PromptConfig::new(
            1,
            vec![],
            vec![fewshot("01A", "示例", "l1_tech", "l2_coding")],
        );
        good.validate(&view).unwrap();
        let mut bad = fewshot("01A", "示例", "l1_tech", "l2_insight");
        bad.output.format_tags.clear();
        let cfg = PromptConfig::new(1, vec![], vec![bad]);
        assert!(cfg.validate(&view).is_err());
    }

    #[test]
    fn repair_messages_append_assistant_and_error_user() {
        let view = tree();
        let msgs = build_classification_messages(&view, &[], &[], &sample());
        let issues = vec![ValidationIssue {
            code: crate::schema::ValidationCode::EnumError,
            detail: "l1_id 不在分类树中".to_string(),
        }];
        let repaired = repair_messages(&msgs, "{}", &issues);
        assert_eq!(repaired.len(), msgs.len() + 2);
        assert_eq!(repaired[msgs.len()].role, "assistant");
        assert!(repaired
            .last()
            .unwrap()
            .content
            .contains("上面的输出未通过校验"));
        assert!(repaired
            .last()
            .unwrap()
            .content
            .contains("请只输出修正后的 JSON 对象"));
    }

    #[test]
    fn template_hash_is_stable_and_short() {
        let cfg = PromptConfig::new(1, vec![], vec![]);
        assert_eq!(cfg.template_hash().len(), 8);
        assert_eq!(cfg.template_hash(), sha256_hex8(&template_source()));
    }
}
