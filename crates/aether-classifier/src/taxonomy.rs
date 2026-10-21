//! 分类树加载（底表 taxonomy.json ⊕ 覆盖层导出的 taxonomy.effective.json）。
//!
//! 规格：#18 §3 / §8.4、#19 §5。评测工具读生效分类树（缺失时回退底表），
//! prompt 注入时自建节点参与、停用 / 合并节点排除、l1_inbox 置末并标「仅用于弃权」。

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde_json::Value;

pub const L1_INBOX: &str = "l1_inbox";
pub const L2_MANUAL_REVIEW: &str = "l2_manual_review";
pub const SELF_BUILT_L2_PREFIX: &str = "l2u_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum L2Status {
    Active,
    Disabled,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatTag {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L2Node {
    pub id: String,
    pub name: String,
    pub preset_tags: Vec<String>,
    pub parent_l1_id: String,
    pub status: L2Status,
    pub self_built: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct L1Node {
    pub id: String,
    pub name: String,
    pub children: Vec<L2Node>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeMapping {
    pub from: String,
    pub to: String,
    pub at: Option<i64>,
}

/// 生效分类树的来源：独立文件（#18）或回退底表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectiveSource {
    File,
    BaseFallback,
}

#[derive(Debug, Clone)]
pub struct TaxonomyView {
    pub base_version: String,
    pub effective_version: String,
    pub effective_source: EffectiveSource,
    pub effective_path: Option<PathBuf>,
    pub merged_into: Vec<MergeMapping>,
    l1s: Vec<L1Node>,
    format_tags: Vec<FormatTag>,
    l1_ids: Vec<String>,
    normal_l1_ids: Vec<String>,
    all_l2_ids: Vec<String>,
    normal_l2_ids: Vec<String>,
    l2_parent: HashMap<String, String>,
    l2_names: HashMap<String, String>,
    l1_names: HashMap<String, String>,
    disabled_l2_ids: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum TaxonomyError {
    #[error("读取分类树失败（{path}）：{source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("解析分类树 JSON 失败（{path}）：{source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("分类树结构非法：{0}")]
    Invalid(String),
}

fn read_json(path: &Path) -> Result<Value, TaxonomyError> {
    let text = std::fs::read_to_string(path).map_err(|source| TaxonomyError::Io {
        path: path.display().to_string(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| TaxonomyError::Json {
        path: path.display().to_string(),
        source,
    })
}

fn as_str<'a>(v: &'a Value, key: &str, ctx: &str) -> Result<&'a str, TaxonomyError> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| TaxonomyError::Invalid(format!("{ctx} 缺少字符串字段 {key}")))
}

/// 生效版本形如 1.1.0+u3；底表版本取 + 之前的部分。
pub fn base_version_of(version: &str) -> String {
    version
        .split('+')
        .next()
        .unwrap_or(version)
        .trim()
        .to_string()
}

pub fn load_base_taxonomy(path: &Path) -> Result<TaxonomyView, TaxonomyError> {
    let doc = read_json(path)?;
    let version = as_str(&doc, "version", "taxonomy")?.to_string();
    build_view(
        &doc,
        version.clone(),
        version,
        EffectiveSource::BaseFallback,
        None,
        vec![],
    )
}

/// 读生效分类树：文件存在且可解析时使用，否则回退底表（#18 §8.4 / #14 §4.1）。
pub fn load_effective_taxonomy(
    base_path: &Path,
    effective_path: Option<&Path>,
) -> Result<TaxonomyView, TaxonomyError> {
    let Some(path) = effective_path else {
        return load_base_taxonomy(base_path);
    };
    if !path.is_file() {
        return load_base_taxonomy(base_path);
    }
    let doc = read_json(path)?;
    let effective_version = as_str(&doc, "version", "taxonomy.effective")?.to_string();
    let base_version = doc
        .get("base_version")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| base_version_of(&effective_version));
    let merged = parse_merged_into(&doc)?;
    build_view(
        &doc,
        base_version,
        effective_version,
        EffectiveSource::File,
        Some(path.to_path_buf()),
        merged,
    )
}

fn parse_merged_into(doc: &Value) -> Result<Vec<MergeMapping>, TaxonomyError> {
    let Some(arr) = doc.get("merged_into") else {
        return Ok(Vec::new());
    };
    let arr = arr
        .as_array()
        .ok_or_else(|| TaxonomyError::Invalid("merged_into 必须是数组".into()))?;
    let mut out = Vec::new();
    for (i, item) in arr.iter().enumerate() {
        let from = as_str(item, "from", &format!("merged_into[{i}]"))?.to_string();
        let to = as_str(item, "to", &format!("merged_into[{i}]"))?.to_string();
        let at = item.get("at").and_then(Value::as_i64);
        out.push(MergeMapping { from, to, at });
    }
    Ok(out)
}

fn build_view(
    doc: &Value,
    base_version: String,
    effective_version: String,
    effective_source: EffectiveSource,
    effective_path: Option<PathBuf>,
    merged_into: Vec<MergeMapping>,
) -> Result<TaxonomyView, TaxonomyError> {
    let taxonomy = doc
        .get("taxonomy")
        .and_then(Value::as_array)
        .ok_or_else(|| TaxonomyError::Invalid("taxonomy 必须是数组".into()))?;
    if taxonomy.is_empty() {
        return Err(TaxonomyError::Invalid("taxonomy 不能为空".into()));
    }

    let mut l1s: Vec<L1Node> = Vec::new();
    let mut l1_ids: Vec<String> = Vec::new();
    let mut normal_l1_ids: Vec<String> = Vec::new();
    let mut all_l2_ids: Vec<String> = Vec::new();
    let mut normal_l2_ids: Vec<String> = Vec::new();
    let mut l2_parent: HashMap<String, String> = HashMap::new();
    let mut l2_names: HashMap<String, String> = HashMap::new();
    let mut l1_names: HashMap<String, String> = HashMap::new();
    let mut disabled_l2_ids: Vec<String> = Vec::new();
    let mut seen_l2: HashSet<String> = HashSet::new();

    for (li, l1) in taxonomy.iter().enumerate() {
        let id = as_str(l1, "id", &format!("taxonomy[{li}]"))?.to_string();
        let name = as_str(l1, "name", &format!("taxonomy[{li}]"))?.to_string();
        if l1_names.contains_key(&id) {
            return Err(TaxonomyError::Invalid(format!("L1 id 重复：{id}")));
        }
        l1_ids.push(id.clone());
        l1_names.insert(id.clone(), name.clone());
        let is_inbox = id == L1_INBOX;
        if !is_inbox {
            normal_l1_ids.push(id.clone());
        }
        let children = l1
            .get("children")
            .and_then(Value::as_array)
            .ok_or_else(|| TaxonomyError::Invalid(format!("L1 {id} 缺少 children 数组")))?;
        let mut child_nodes = Vec::new();
        for (ci, child) in children.iter().enumerate() {
            let ctx = format!("L1 {id} children[{ci}]");
            let child_id = as_str(child, "id", &ctx)?.to_string();
            let child_name = as_str(child, "name", &ctx)?.to_string();
            if !seen_l2.insert(child_id.clone()) {
                return Err(TaxonomyError::Invalid(format!("L2 id 重复：{child_id}")));
            }
            let status = match child.get("status").and_then(Value::as_str) {
                None | Some("active") => L2Status::Active,
                Some("disabled") => L2Status::Disabled,
                Some("merged") => L2Status::Merged,
                Some(other) => {
                    return Err(TaxonomyError::Invalid(format!(
                        "{ctx} status 非法：{other}（允许 active/disabled/merged）"
                    )))
                }
            };
            if status != L2Status::Active {
                disabled_l2_ids.push(child_id.clone());
                continue;
            }
            let preset_tags = child
                .get("l3_preset_tags")
                .and_then(Value::as_array)
                .map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let self_built = child_id.starts_with(SELF_BUILT_L2_PREFIX);
            all_l2_ids.push(child_id.clone());
            if !is_inbox {
                normal_l2_ids.push(child_id.clone());
            }
            l2_parent.insert(child_id.clone(), id.clone());
            l2_names.insert(child_id.clone(), child_name.clone());
            child_nodes.push(L2Node {
                id: child_id,
                name: child_name,
                preset_tags,
                parent_l1_id: id.clone(),
                status,
                self_built,
            });
        }
        l1s.push(L1Node {
            id,
            name,
            children: child_nodes,
        });
    }

    if !l1_ids.iter().any(|id| id == L1_INBOX) {
        return Err(TaxonomyError::Invalid("缺少 l1_inbox（00 未分类）".into()));
    }
    if !all_l2_ids.iter().any(|id| id == L2_MANUAL_REVIEW) {
        return Err(TaxonomyError::Invalid(
            "缺少 l2_manual_review（待人工归档）或该节点被停用".into(),
        ));
    }
    if l2_parent.get(L2_MANUAL_REVIEW).map(String::as_str) != Some(L1_INBOX) {
        return Err(TaxonomyError::Invalid(
            "l2_manual_review 必须挂在 l1_inbox 下".into(),
        ));
    }

    let mut format_tags = Vec::new();
    if let Some(arr) = doc
        .get("global_l3_tags")
        .and_then(|v| v.get("format_tags"))
        .and_then(Value::as_array)
    {
        for (i, tag) in arr.iter().enumerate() {
            format_tags.push(FormatTag {
                id: as_str(tag, "id", &format!("global_l3_tags.format_tags[{i}]"))?.to_string(),
                name: tag
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }
    if format_tags.is_empty() {
        return Err(TaxonomyError::Invalid(
            "global_l3_tags.format_tags 不能为空".into(),
        ));
    }

    Ok(TaxonomyView {
        base_version,
        effective_version,
        effective_source,
        effective_path,
        merged_into,
        l1s,
        format_tags,
        l1_ids,
        normal_l1_ids,
        all_l2_ids,
        normal_l2_ids,
        l2_parent,
        l2_names,
        l1_names,
        disabled_l2_ids,
    })
}

impl TaxonomyView {
    pub fn l1s(&self) -> &[L1Node] {
        &self.l1s
    }

    pub fn normal_l1_ids(&self) -> &[String] {
        &self.normal_l1_ids
    }

    pub fn normal_l2_ids(&self) -> &[String] {
        &self.normal_l2_ids
    }

    pub fn all_l1_ids(&self) -> &[String] {
        &self.l1_ids
    }

    pub fn all_l2_ids(&self) -> &[String] {
        &self.all_l2_ids
    }

    pub fn disabled_l2_ids(&self) -> &[String] {
        &self.disabled_l2_ids
    }

    pub fn format_tags(&self) -> &[FormatTag] {
        &self.format_tags
    }

    pub fn is_normal_l1(&self, id: &str) -> bool {
        self.normal_l1_ids.iter().any(|x| x == id)
    }

    pub fn is_normal_l2(&self, id: &str) -> bool {
        self.normal_l2_ids.iter().any(|x| x == id)
    }

    pub fn is_any_l2(&self, id: &str) -> bool {
        self.all_l2_ids.iter().any(|x| x == id)
    }

    pub fn is_format_tag(&self, id: &str) -> bool {
        self.format_tags.iter().any(|t| t.id == id)
    }

    pub fn l2_parent(&self, l2_id: &str) -> Option<&str> {
        self.l2_parent.get(l2_id).map(String::as_str)
    }

    pub fn l2_name(&self, l2_id: &str) -> Option<&str> {
        self.l2_names.get(l2_id).map(String::as_str)
    }

    pub fn l1_name(&self, l1_id: &str) -> Option<&str> {
        self.l1_names.get(l1_id).map(String::as_str)
    }

    pub fn is_abstention(&self, l1_id: &str, l2_id: &str) -> bool {
        l1_id == L1_INBOX && l2_id == L2_MANUAL_REVIEW
    }

    pub fn normal_l2_names(&self) -> Vec<(&str, &str)> {
        self.normal_l2_ids
            .iter()
            .map(|id| (id.as_str(), self.l2_name(id).unwrap_or("")))
            .collect()
    }

    /// 自建且启用中的 L2（未覆盖节点统计用）。
    pub fn self_built_l2_ids(&self) -> Vec<&str> {
        self.l2s()
            .filter(|n| n.self_built)
            .map(|n| n.id.as_str())
            .collect()
    }

    pub fn l2s(&self) -> impl Iterator<Item = &L2Node> {
        self.l1s.iter().flat_map(|l1| l1.children.iter())
    }

    /// 合并映射链（用于评测集金标重映射）：l2u_old → l2_official → …；检测环。
    pub fn remap_l2_id(&self, l2_id: &str) -> Option<String> {
        if self.merged_into.is_empty() {
            return None;
        }
        let mut seen = HashSet::new();
        let mut current = l2_id.to_string();
        let mut remapped = false;
        loop {
            if !seen.insert(current.clone()) {
                return None; // 环，忽略
            }
            match self.merged_into.iter().find(|m| m.from == current) {
                Some(m) => {
                    current = m.to.clone();
                    remapped = true;
                }
                None => break,
            }
        }
        if remapped {
            Some(current)
        } else {
            None
        }
    }

    /// 把生效分类树渲染成 prompt 的【分类树】段（#19 §5）。
    pub fn render_tree(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        let inbox: Vec<&L1Node> = self.l1s.iter().filter(|l| l.id == L1_INBOX).collect();
        let mut ordered: Vec<&L1Node> = self.l1s.iter().filter(|l| l.id != L1_INBOX).collect();
        ordered.extend(inbox);
        for l1 in ordered {
            if l1.id == L1_INBOX {
                lines.push(format!("{} {}（仅用于弃权）", l1.id, l1.name));
            } else {
                lines.push(format!("{} {}", l1.id, l1.name));
            }
            for child in &l1.children {
                let preset = if child.preset_tags.is_empty() {
                    String::new()
                } else {
                    format!("｜预设标签：{}", child.preset_tags.join("、"))
                };
                lines.push(format!("  · {}（{}）{}", child.name, child.id, preset));
            }
        }
        lines.join("\n")
    }

    /// 全部正常 L2（12 个官方 + 启用中的自建）用于 prompt 的「共 N 个」。
    pub fn normal_l2_count(&self) -> usize {
        self.normal_l2_ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn write_tmp(name: &str, content: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "aether-tax-test-{}-{}",
            std::process::id(),
            fastrand::u64(..)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, content).unwrap();
        path
    }

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
          {"id":"l2_coding","name":"编程与低代码","l3_preset_tags":["Cursor","Rust"]},
          {"id":"l2_prompt_agent","name":"提示工程与智能体","l3_preset_tags":["Coze"]}
        ]},
        {"id":"l1_biz","name":"04 商业与认知","children":[
          {"id":"l2_insight","name":"行业观察与研报","l3_preset_tags":[]}
        ]}
      ]
    }"#;

    #[test]
    fn base_loads_and_renders_inbox_last() {
        let path = write_tmp("taxonomy.json", BASE);
        let view = load_effective_taxonomy(&path, None).unwrap();
        assert_eq!(view.base_version, "1.1.0");
        assert_eq!(view.effective_version, "1.1.0");
        assert_eq!(view.effective_source, EffectiveSource::BaseFallback);
        assert_eq!(view.normal_l1_ids().len(), 2);
        assert_eq!(view.normal_l2_ids().len(), 3);
        assert!(view.is_normal_l2("l2_coding"));
        assert!(!view.is_normal_l2("l2_manual_review"));
        let tree = view.render_tree();
        assert!(
            tree.ends_with("l1_inbox 00 未分类（仅用于弃权）\n  · 待人工归档（l2_manual_review）")
        );
        assert!(tree.find("l1_tech").unwrap() < tree.find("l1_inbox").unwrap());
        assert!(tree.contains("· 编程与低代码（l2_coding）｜预设标签：Cursor、Rust"));
    }

    #[test]
    fn effective_tree_supports_self_built_disabled_and_merged() {
        let base_path = write_tmp("base.json", BASE);
        let eff = r#"{
          "version": "1.1.0+u3",
          "base_version": "1.1.0",
          "merged_into": [{"from":"l2u_old","to":"l2_insight","at":1761000000000}],
          "global_l3_tags": {"format_tags": [{"id":"tag_tool","name":"工具"}]},
          "taxonomy": [
            {"id":"l1_inbox","name":"00 未分类","children":[{"id":"l2_manual_review","name":"待人工归档"}]},
            {"id":"l1_tech","name":"03 技术与工程","children":[
              {"id":"l2_coding","name":"编程与低代码"},
              {"id":"l2_prompt_agent","name":"提示工程与智能体","status":"disabled"}
            ]},
            {"id":"l1_biz","name":"04 商业与认知","children":[
              {"id":"l2_insight","name":"行业观察与研报"},
              {"id":"l2u_01AAA","name":"交易与投资","l3_preset_tags":["Polymarket"]}
            ]}
          ]
        }"#;
        let eff_path = write_tmp("taxonomy.effective.json", eff);
        let view = load_effective_taxonomy(&base_path, Some(&eff_path)).unwrap();
        assert_eq!(view.effective_version, "1.1.0+u3");
        assert_eq!(view.base_version, "1.1.0");
        assert_eq!(view.effective_source, EffectiveSource::File);
        assert!(view
            .disabled_l2_ids()
            .contains(&"l2_prompt_agent".to_string()));
        assert!(view.is_normal_l2("l2u_01AAA"));
        assert_eq!(view.self_built_l2_ids(), vec!["l2u_01AAA"]);
        assert_eq!(view.remap_l2_id("l2u_old").as_deref(), Some("l2_insight"));
        assert_eq!(view.remap_l2_id("l2_coding"), None);
        assert!(view
            .render_tree()
            .contains("交易与投资（l2u_01AAA）｜预设标签：Polymarket"));
    }

    #[test]
    fn missing_effective_file_falls_back_to_base() {
        let base = write_tmp("taxonomy.json", BASE);
        let view = load_effective_taxonomy(&base, Some(&base.with_file_name("nope.json"))).unwrap();
        assert_eq!(view.effective_source, EffectiveSource::BaseFallback);
    }

    #[test]
    fn rejects_duplicate_l2_ids() {
        let broken = BASE.replace(
            "{\"id\":\"l2_insight\",\"name\":\"行业观察与研报\",\"l3_preset_tags\":[]}",
            "{\"id\":\"l2_coding\",\"name\":\"重复\",\"l3_preset_tags\":[]}",
        );
        let path = write_tmp("taxonomy.json", &broken);
        let err = load_base_taxonomy(&path).unwrap_err();
        assert!(err.to_string().contains("L2 id 重复"));
    }
}
