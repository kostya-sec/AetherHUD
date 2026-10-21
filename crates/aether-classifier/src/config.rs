//! 本地分类器配置：few-shot 实例池与特征标签候选池快照。
//!
//! 真实样本只存本地 app_data_dir（#14 §3.4 / #19 §9），不入 git；
//! 仓库仅保留脱敏模板与加载/校验逻辑。

use std::path::Path;

use serde_json::Value;

use crate::prompt::FewShotExample;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("读取配置失败（{path}）：{source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("解析配置 JSON 失败（{path}）：{source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("配置结构非法（{path}）：{message}")]
    Invalid { path: String, message: String },
}

fn read_json(path: &Path) -> Result<Value, ConfigError> {
    let text = std::fs::read_to_string(path).map_err(|source| ConfigError::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| ConfigError::Json {
        path: path.display().to_string(),
        source,
    })
}

/// 读取 few-shot 配置：JSON 数组，或 {"few_shots": [...]}。
pub fn load_few_shots(path: &Path) -> Result<Vec<FewShotExample>, ConfigError> {
    let value = read_json(path)?;
    let arr = value
        .as_array()
        .or_else(|| value.get("few_shots").and_then(Value::as_array))
        .ok_or_else(|| ConfigError::Invalid {
            path: path.display().to_string(),
            message: "应为 few-shot 数组或 {\"few_shots\": [...]}".to_string(),
        })?;
    serde_json::from_value(Value::Array(arr.clone())).map_err(|source| ConfigError::Invalid {
        path: path.display().to_string(),
        message: format!("few-shot 字段不合法：{source}"),
    })
}

/// 读取特征标签候选池快照：JSON 数组，或 {"tags": [...]}，或 {"pool": [...]}。
/// 生产环境由 DB 高频标签快照生成（#19 §6.2）；评测工具读本地快照。
pub fn load_tag_pool(path: &Path) -> Result<Vec<String>, ConfigError> {
    let value = read_json(path)?;
    let arr = value
        .as_array()
        .or_else(|| value.get("tags").and_then(Value::as_array))
        .or_else(|| value.get("pool").and_then(Value::as_array))
        .ok_or_else(|| ConfigError::Invalid {
            path: path.display().to_string(),
            message: "应为标签数组或 {\"tags\": [...]}".to_string(),
        })?;
    let mut out: Vec<String> = Vec::new();
    for item in arr {
        let Some(tag) = item.as_str() else {
            return Err(ConfigError::Invalid {
                path: path.display().to_string(),
                message: "标签候选池元素必须是字符串".to_string(),
            });
        };
        let tag = tag.trim();
        if tag.is_empty() || out.iter().any(|t| t == tag) {
            continue;
        }
        out.push(tag.to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_tmp(content: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "aether-config-test-{}-{}.json",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn loads_few_shots_array_and_object() {
        let few = r#"[{"entry_id":"01A","input":{"title":"t","url":"","media_type":"link","body_excerpt":"b"},"output":{"l1_id":"l1_tech","l2_id":"l2_coding","format_tags":["tag_tool"],"feature_tags":[]}}]"#;
        let arr = load_few_shots(&write_tmp(few)).unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].entry_id.as_deref(), Some("01A"));
        let wrapped = format!("{{\"few_shots\":{few}}}");
        assert_eq!(load_few_shots(&write_tmp(&wrapped)).unwrap().len(), 1);
    }

    #[test]
    fn tag_pool_trims_dedupes_and_accepts_wrappers() {
        let path = write_tmp(r#"{"tags":["Cursor"," Cursor ","","Rust"]}"#);
        assert_eq!(load_tag_pool(&path).unwrap(), vec!["Cursor", "Rust"]);
        let plain = write_tmp(r#"["A","B"]"#);
        assert_eq!(load_tag_pool(&plain).unwrap(), vec!["A", "B"]);
    }

    #[test]
    fn invalid_structure_reports_path() {
        let err = load_few_shots(&write_tmp(r#"{"nope":1}"#)).unwrap_err();
        assert!(err.to_string().contains("few-shot"));
    }
}
