//! 分类输出的逐字段校验与归一化（#19 §8.2 / #18 §2.2）。
//!
//! 生产分类路径与评测工具共用本函数：校验失败 = 分类失败（格式非法），
//! 不计入准确率分母（#14 §5）；弃权是合法输出，suggestion 逐字段降级、不判整条失败。

use serde::{Deserialize, Serialize};
use serde_json::Value;
use unicode_normalization::UnicodeNormalization;

use crate::taxonomy::{TaxonomyView, L1_INBOX, L2_MANUAL_REVIEW};

pub const FORMAT_TAGS_MIN: usize = 1;
pub const FORMAT_TAGS_MAX: usize = 2;
pub const FEATURE_TAGS_MAX: usize = 3;
pub const FEATURE_TAG_MAX_CHARS: usize = 20;
pub const NEW_L2_NAME_MIN_CHARS: usize = 2;
pub const NEW_L2_NAME_MAX_CHARS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    pub l1_id: String,
    pub l2_id: String,
    pub format_tags: Vec<String>,
    pub feature_tags: Vec<String>,
    #[serde(default)]
    pub suggestion: Option<Suggestion>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Suggestion {
    #[serde(default)]
    pub nearest_l2_id: Option<String>,
    #[serde(default)]
    pub new_l2: Option<NewL2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewL2 {
    pub parent_l1_id: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationCode {
    ParseError,
    SchemaError,
    EnumError,
    FormatTagError,
    FeatureTagError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationIssue {
    pub code: ValidationCode,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationError {
    pub issues: Vec<ValidationIssue>,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "分类输出校验失败（{} 项）：", self.issues.len())?;
        for (i, issue) in self.issues.iter().enumerate() {
            if i > 0 {
                write!(f, "；")?;
            }
            write!(f, "{:?}: {}", issue.code, issue.detail)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationOutcome {
    pub classification: Classification,
    pub abstained: bool,
    /// 非致命降级记录（suggestion 字段丢弃等），写日志用，不参与失败判定。
    pub notes: Vec<String>,
}

fn issue(code: ValidationCode, detail: impl Into<String>) -> ValidationIssue {
    ValidationIssue {
        code,
        detail: detail.into(),
    }
}

fn err(issues: Vec<ValidationIssue>) -> ValidationError {
    ValidationError { issues }
}

/// 特征标签归一化 key：NFKC、trim、连续空白折叠、小写（#19 §8.2 第 5 步）。
pub fn normalized_feature_key(tag: &str) -> String {
    let nfkc: String = tag.nfkc().collect();
    let collapsed = normalize_ws(&nfkc);
    collapsed.to_lowercase()
}

fn normalize_ws(s: &str) -> String {
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

fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// 唯一入口：解析 + 分层校验 + 归一化。
pub fn validate_classification(
    raw: &str,
    tree: &TaxonomyView,
) -> Result<ValidationOutcome, ValidationError> {
    let value: Value = match serde_json::from_str(raw) {
        Ok(v) => v,
        Err(e) => {
            return Err(err(vec![issue(
                ValidationCode::ParseError,
                format!("JSON 解析失败：{e}"),
            )]))
        }
    };
    validate_classification_value(&value, tree)
}

pub fn validate_classification_value(
    value: &Value,
    tree: &TaxonomyView,
) -> Result<ValidationOutcome, ValidationError> {
    let Some(obj) = value.as_object() else {
        return Err(err(vec![issue(
            ValidationCode::SchemaError,
            "输出必须是 JSON 对象".to_string(),
        )]));
    };

    let mut schema_issues = Vec::new();
    let l1_id = obj.get("l1_id").and_then(Value::as_str).map(str::trim);
    if l1_id.is_none() {
        schema_issues.push(issue(ValidationCode::SchemaError, "l1_id 缺失或不是字符串"));
    }
    let l2_id = obj.get("l2_id").and_then(Value::as_str).map(str::trim);
    if l2_id.is_none() {
        schema_issues.push(issue(ValidationCode::SchemaError, "l2_id 缺失或不是字符串"));
    }
    let format_tags_raw = match obj.get("format_tags") {
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for (i, item) in arr.iter().enumerate() {
                match item.as_str() {
                    Some(s) => out.push(s.to_string()),
                    None => schema_issues.push(issue(
                        ValidationCode::SchemaError,
                        format!("format_tags[{i}] 不是字符串"),
                    )),
                }
            }
            Some(out)
        }
        Some(_) => {
            schema_issues.push(issue(
                ValidationCode::SchemaError,
                "format_tags 缺失或不是数组",
            ));
            None
        }
        None => {
            schema_issues.push(issue(ValidationCode::SchemaError, "format_tags 缺失"));
            None
        }
    };
    let feature_tags_raw = match obj.get("feature_tags") {
        Some(Value::Array(arr)) => {
            let mut out = Vec::with_capacity(arr.len());
            for (i, item) in arr.iter().enumerate() {
                match item.as_str() {
                    Some(s) => out.push(s.to_string()),
                    None => schema_issues.push(issue(
                        ValidationCode::SchemaError,
                        format!("feature_tags[{i}] 不是字符串"),
                    )),
                }
            }
            Some(out)
        }
        Some(_) => {
            schema_issues.push(issue(
                ValidationCode::SchemaError,
                "feature_tags 缺失或不是数组",
            ));
            None
        }
        None => {
            schema_issues.push(issue(ValidationCode::SchemaError, "feature_tags 缺失"));
            None
        }
    };
    let suggestion_value = obj.get("suggestion").cloned().unwrap_or(Value::Null);
    let suggestion_is_object = suggestion_value.is_object();
    if !suggestion_value.is_null() && !suggestion_is_object {
        schema_issues.push(issue(
            ValidationCode::SchemaError,
            "suggestion 必须是对象或 null",
        ));
    }
    if !schema_issues.is_empty() {
        return Err(err(schema_issues));
    }

    let l1_id = l1_id.unwrap_or_default().to_string();
    let l2_id = l2_id.unwrap_or_default().to_string();
    let mut enum_issues = Vec::new();
    if !tree.all_l1_ids().iter().any(|id| id == &l1_id) {
        enum_issues.push(issue(
            ValidationCode::EnumError,
            format!("l1_id 不在分类树中：{l1_id}"),
        ));
    }
    if !tree.is_any_l2(&l2_id) {
        enum_issues.push(issue(
            ValidationCode::EnumError,
            format!("l2_id 不在分类树中：{l2_id}"),
        ));
    }
    if tree.all_l1_ids().iter().any(|id| id == &l1_id) && tree.is_any_l2(&l2_id) {
        if tree.l2_parent(&l2_id) != Some(l1_id.as_str()) {
            enum_issues.push(issue(
                ValidationCode::EnumError,
                format!("l2_id {l2_id} 不属于 l1_id {l1_id}"),
            ));
        }
        if l2_id == L2_MANUAL_REVIEW && l1_id != L1_INBOX {
            enum_issues.push(issue(
                ValidationCode::EnumError,
                "l2_manual_review 只能配 l1_inbox".to_string(),
            ));
        }
        if l1_id == L1_INBOX && l2_id != L2_MANUAL_REVIEW {
            enum_issues.push(issue(
                ValidationCode::EnumError,
                "弃权必须输出 l1_inbox + l2_manual_review".to_string(),
            ));
        }
    }
    if !enum_issues.is_empty() {
        return Err(err(enum_issues));
    }

    let mut notes = Vec::new();
    let mut format_issues = Vec::new();
    let mut format_tags: Vec<String> = Vec::new();
    for raw_tag in format_tags_raw.unwrap_or_default() {
        let tag = raw_tag.trim();
        if tag.is_empty() {
            notes.push("format_tags 中的空字符串已丢弃".to_string());
            continue;
        }
        if !tree.is_format_tag(tag) {
            format_issues.push(issue(
                ValidationCode::FormatTagError,
                format!("format_tag 非法：{tag}"),
            ));
            continue;
        }
        if !format_tags.iter().any(|t| t == tag) {
            format_tags.push(tag.to_string());
        }
    }
    if !(FORMAT_TAGS_MIN..=FORMAT_TAGS_MAX).contains(&format_tags.len()) {
        format_issues.push(issue(
            ValidationCode::FormatTagError,
            format!("format_tags 必须 1~2 个，实际 {} 个", format_tags.len()),
        ));
    }
    if !format_issues.is_empty() {
        return Err(err(format_issues));
    }

    let mut feature_issues = Vec::new();
    let mut feature_tags: Vec<String> = Vec::new();
    let mut seen_feature_keys = Vec::new();
    let raw_features = feature_tags_raw.unwrap_or_default();
    if raw_features.len() > FEATURE_TAGS_MAX {
        feature_issues.push(issue(
            ValidationCode::FeatureTagError,
            format!(
                "feature_tags 最多 {FEATURE_TAGS_MAX} 个，实际 {} 个",
                raw_features.len()
            ),
        ));
    }
    for raw_tag in raw_features {
        let tag = raw_tag.trim();
        if tag.is_empty() {
            feature_issues.push(issue(
                ValidationCode::FeatureTagError,
                "feature_tags 含空字符串".to_string(),
            ));
            continue;
        }
        if char_len(tag) > FEATURE_TAG_MAX_CHARS {
            feature_issues.push(issue(
                ValidationCode::FeatureTagError,
                format!("feature_tag 长度超过 {FEATURE_TAG_MAX_CHARS} 字：{tag}"),
            ));
        }
        if tag.chars().any(char::is_control) {
            feature_issues.push(issue(
                ValidationCode::FeatureTagError,
                format!("feature_tag 含控制字符：{tag:?}"),
            ));
        }
        let key = normalized_feature_key(tag);
        if seen_feature_keys.contains(&key) {
            notes.push(format!("feature_tag 归一化后重复，已去重：{tag}"));
            continue;
        }
        seen_feature_keys.push(key);
        feature_tags.push(tag.to_string());
    }
    if !feature_issues.is_empty() {
        return Err(err(feature_issues));
    }

    let abstained = l1_id == L1_INBOX && l2_id == L2_MANUAL_REVIEW;
    let suggestion = normalize_suggestion(&suggestion_value, tree, abstained, &mut notes);

    Ok(ValidationOutcome {
        classification: Classification {
            l1_id,
            l2_id,
            format_tags,
            feature_tags,
            suggestion,
        },
        abstained,
        notes,
    })
}

fn normalize_suggestion(
    value: &Value,
    tree: &TaxonomyView,
    abstained: bool,
    notes: &mut Vec<String>,
) -> Option<Suggestion> {
    if value.is_null() {
        if abstained {
            notes.push("弃权未给 suggestion（允许，仅减少人工线索）".to_string());
        }
        return None;
    }
    if !abstained {
        notes.push("非弃权输出带 suggestion，已忽略".to_string());
        return None;
    }
    let obj = value.as_object()?;
    let mut suggestion = Suggestion {
        nearest_l2_id: None,
        new_l2: None,
    };
    match obj.get("nearest_l2_id") {
        None | Some(Value::Null) => {}
        Some(Value::String(s)) => {
            let id = s.trim();
            if tree.is_normal_l2(id) {
                suggestion.nearest_l2_id = Some(id.to_string());
            } else {
                notes.push(format!("nearest_l2_id 非生效树中的正常 L2，已置空：{id}"));
            }
        }
        Some(_) => notes.push("nearest_l2_id 类型非法，已置空".to_string()),
    }
    match obj.get("new_l2") {
        None | Some(Value::Null) => {}
        Some(Value::Object(new_obj)) => {
            let parent = new_obj
                .get("parent_l1_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            let name = new_obj
                .get("name")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            let parent_ok = tree.is_normal_l1(parent);
            let name_len = char_len(name);
            let name_ok = (NEW_L2_NAME_MIN_CHARS..=NEW_L2_NAME_MAX_CHARS).contains(&name_len)
                && !name
                    .chars()
                    .any(|c| c.is_control() || c == '/' || c == '\\');
            if parent_ok && name_ok {
                suggestion.new_l2 = Some(NewL2 {
                    parent_l1_id: parent.to_string(),
                    name: name.to_string(),
                });
            } else {
                notes.push(format!(
                    "new_l2 字段非法（parent_l1_id={parent:?}, name={name:?}），已丢弃"
                ));
            }
        }
        Some(_) => notes.push("new_l2 类型非法，已丢弃".to_string()),
    }
    if suggestion.nearest_l2_id.is_none() && suggestion.new_l2.is_none() {
        None
    } else {
        Some(suggestion)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::taxonomy::load_base_taxonomy;
    use std::path::PathBuf;

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
        let path: PathBuf = std::env::temp_dir().join(format!(
            "aether-schema-test-{}-{}.json",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::write(&path, BASE).unwrap();
        load_base_taxonomy(&path).unwrap()
    }

    fn codes(e: &ValidationError) -> Vec<ValidationCode> {
        e.issues.iter().map(|i| i.code).collect()
    }

    #[test]
    fn accepts_valid_output() {
        let out = validate_classification(
            r#"{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":["Cursor"],"suggestion":null}"#,
            &tree(),
        )
        .unwrap();
        assert!(!out.abstained);
        assert_eq!(out.classification.format_tags, vec!["tag_tool"]);
        assert_eq!(out.classification.feature_tags, vec!["Cursor"]);
    }

    #[test]
    fn parse_error_is_format_invalid() {
        let e = validate_classification("not json", &tree()).unwrap_err();
        assert_eq!(codes(&e), vec![ValidationCode::ParseError]);
    }

    #[test]
    fn unknown_l1_is_enum_error() {
        let e = validate_classification(
            r#"{"l1_id":"l1_nope","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":[]}"#,
            &tree(),
        )
        .unwrap_err();
        assert!(codes(&e).contains(&ValidationCode::EnumError));
    }

    #[test]
    fn l2_parent_mismatch_is_enum_error() {
        let e = validate_classification(
            r#"{"l1_id":"l1_tech","l2_id":"l2_insight","format_tags":["tag_tool"],"feature_tags":[]}"#,
            &tree(),
        )
        .unwrap_err();
        assert!(codes(&e).contains(&ValidationCode::EnumError));
    }

    #[test]
    fn format_tag_count_and_legality() {
        let e = validate_classification(
            r#"{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool","tag_asset","tag_prompt"],"feature_tags":[]}"#,
            &tree(),
        )
        .unwrap_err();
        assert!(codes(&e).contains(&ValidationCode::FormatTagError));
        let e2 = validate_classification(
            r#"{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_bogus"],"feature_tags":[]}"#,
            &tree(),
        )
        .unwrap_err();
        assert!(codes(&e2).contains(&ValidationCode::FormatTagError));
    }

    #[test]
    fn feature_tags_too_many_and_too_long() {
        let e = validate_classification(
            r#"{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":["a","b","c","d"]}"#,
            &tree(),
        )
        .unwrap_err();
        assert!(codes(&e).contains(&ValidationCode::FeatureTagError));
        let long = "x".repeat(21);
        let e2 = validate_classification(
            &format!(
                r#"{{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":["{long}"]}}"#
            ),
            &tree(),
        )
        .unwrap_err();
        assert!(codes(&e2).contains(&ValidationCode::FeatureTagError));
    }

    #[test]
    fn feature_tags_dedupe_after_normalization() {
        let out = validate_classification(
            r#"{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":["Cursor","cursor","Ｃｕｒｓｏｒ"]}"#,
            &tree(),
        )
        .unwrap();
        assert_eq!(out.classification.feature_tags, vec!["Cursor"]);
    }

    #[test]
    fn abstention_accepts_and_degrades_suggestion() {
        let out = validate_classification(
            r#"{"l1_id":"l1_inbox","l2_id":"l2_manual_review","format_tags":["tag_tool"],"feature_tags":[],"suggestion":{"nearest_l2_id":"l2_insight","new_l2":{"parent_l1_id":"l1_biz","name":"交易与投资"}}}"#,
            &tree(),
        )
        .unwrap();
        assert!(out.abstained);
        let s = out.classification.suggestion.unwrap();
        assert_eq!(s.nearest_l2_id.as_deref(), Some("l2_insight"));
        assert_eq!(s.new_l2.unwrap().name, "交易与投资");

        let out = validate_classification(
            r#"{"l1_id":"l1_inbox","l2_id":"l2_manual_review","format_tags":["tag_tool"],"feature_tags":[],"suggestion":{"nearest_l2_id":"l2_bad","new_l2":{"parent_l1_id":"l1_inbox","name":"x"}}}"#,
            &tree(),
        )
        .unwrap();
        assert!(out.classification.suggestion.is_none());
        assert!(out.notes.iter().any(|n| n.contains("nearest_l2_id")));
    }

    #[test]
    fn non_abstain_suggestion_is_ignored_not_fatal() {
        let out = validate_classification(
            r#"{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":[],"suggestion":{"nearest_l2_id":"l2_insight"}}"#,
            &tree(),
        )
        .unwrap();
        assert!(out.classification.suggestion.is_none());
        assert!(out.notes.iter().any(|n| n.contains("非弃权")));
    }

    #[test]
    fn partial_abstention_is_enum_error() {
        let e = validate_classification(
            r#"{"l1_id":"l1_inbox","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":[]}"#,
            &tree(),
        )
        .unwrap_err();
        assert!(codes(&e).contains(&ValidationCode::EnumError));
    }
}
