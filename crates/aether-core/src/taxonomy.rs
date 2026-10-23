//! 生效分类树：只读底表 ⊕ 用户覆盖层 → 合并、校验、版本与导出（#27 / #18 §3）。
//!
//! 唯一实现：分类器与评测工具不各自读文件、不各自合并，只消费这里产出的
//! TaxonomyView（aether-classifier::taxonomy）。启动流程：
//! 读底表 → 读/建覆盖层 → 校验 → 合并 → 原子写 taxonomy.effective.json → 构造视图。

use std::collections::{HashMap, HashSet};
use std::io;
use std::path::{Path, PathBuf};

use aether_classifier::schema::is_valid_new_l2_name;
use aether_classifier::taxonomy::{
    base_view_from_json, effective_view_from_json, normalize_name, TaxonomyView,
    SELF_BUILT_L2_PREFIX,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::adapters::{Clock, FileSystem};
use crate::notice::HudNotice;

pub const OVERLAY_FILE: &str = "taxonomy.overlay.json";
pub const EFFECTIVE_FILE: &str = "taxonomy.effective.json";
pub const CORRUPT_OVERLAY_PREFIX: &str = "taxonomy.overlay.corrupt-";
pub const OVERLAY_FORMAT_VERSION: u32 = 1;

/// 分类树相关路径的统一来源（数据目录由壳/CLI 注入）。
#[derive(Debug, Clone)]
pub struct TaxonomyPaths {
    data_dir: PathBuf,
    base: PathBuf,
}

impl TaxonomyPaths {
    pub fn new(data_dir: impl Into<PathBuf>, base: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            base: base.into(),
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn base_path(&self) -> &Path {
        &self.base
    }

    pub fn overlay_path(&self) -> PathBuf {
        self.data_dir.join(OVERLAY_FILE)
    }

    pub fn effective_path(&self) -> PathBuf {
        self.data_dir.join(EFFECTIVE_FILE)
    }

    /// 覆盖层损坏快照：taxonomy.overlay.corrupt-<ts>.json（#18 §3.5 第 5 步）。
    pub fn corrupt_overlay_path(&self, now_ms: i64) -> PathBuf {
        self.data_dir
            .join(format!("{CORRUPT_OVERLAY_PREFIX}{now_ms}.json"))
    }
}

/// 覆盖层文件格式（#18 §3.3）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayDocument {
    pub format_version: u32,
    pub base_version_seen: String,
    pub revision: u32,
    #[serde(default)]
    pub nodes: Vec<OverlayNode>,
    #[serde(default)]
    pub merged_into: Vec<OverlayMerge>,
}

impl OverlayDocument {
    pub fn empty(base_version: &str) -> Self {
        Self {
            format_version: OVERLAY_FORMAT_VERSION,
            base_version_seen: base_version.to_string(),
            revision: 0,
            nodes: Vec::new(),
            merged_into: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayNode {
    pub id: String,
    pub parent_l1_id: String,
    pub name: String,
    #[serde(default)]
    pub l3_preset_tags: Vec<String>,
    #[serde(default)]
    pub status: OverlayNodeStatus,
    pub created_at: i64,
    #[serde(default)]
    pub renamed_at: Option<i64>,
    #[serde(default)]
    pub disabled_at: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayNodeStatus {
    #[default]
    Active,
    Disabled,
    Merged,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverlayMerge {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub at: Option<i64>,
}

/// 启动结果：生效视图 + 覆盖层状态 + 需要 HUD 提示的事件。
#[derive(Debug, Clone)]
pub struct TaxonomyActivation {
    pub view: TaxonomyView,
    pub overlay_revision: u32,
    pub overlay_created: bool,
    pub notices: Vec<HudNotice>,
}

impl TaxonomyActivation {
    pub fn base_version(&self) -> &str {
        &self.view.base_version
    }

    pub fn effective_version(&self) -> &str {
        &self.view.effective_version
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TaxonomyStartupError {
    #[error("读取底表分类树失败（{path}）：{source}")]
    BaseRead {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("底表分类树非法：{0}")]
    BaseInvalid(aether_classifier::taxonomy::TaxonomyError),
    #[error("创建数据目录失败（{path}）：{source}")]
    DataDir {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("读取覆盖层失败（{path}）：{source}")]
    OverlayRead {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("备份覆盖层失败（{path}）：{source}")]
    OverlayBackup {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("写覆盖层失败（{path}）：{source}")]
    OverlayWrite {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("写生效分类树失败（{path}）：{source}")]
    EffectiveWrite {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("生效分类树非法：{0}")]
    EffectiveInvalid(aether_classifier::taxonomy::TaxonomyError),
    #[error("序列化分类树失败：{0}")]
    Serialize(String),
}

/// 评测/校验脚本的只读加载：读应用导出的生效树，缺失时回退底表；不合并、不写盘。
///
/// 生产启动走 activate；本函数只消费已经由应用导出的 taxonomy.effective.json，
/// 保证评测读到的分类树与运行期一致（#18 §8.4 / #24 测试决策）。
pub fn load_exported_view(
    fs: &dyn FileSystem,
    base_path: &Path,
    effective_path: Option<&Path>,
) -> Result<TaxonomyView, TaxonomyReadError> {
    let base_text = fs
        .read_to_string(base_path)
        .map_err(|source| TaxonomyReadError::BaseRead {
            path: base_path.display().to_string(),
            source,
        })?;
    let base_view = base_view_from_json(&base_text).map_err(TaxonomyReadError::BaseInvalid)?;
    let Some(effective_path) = effective_path else {
        return Ok(base_view);
    };
    if !fs.exists(effective_path) {
        return Ok(base_view);
    }
    let effective_text =
        fs.read_to_string(effective_path)
            .map_err(|source| TaxonomyReadError::EffectiveRead {
                path: effective_path.display().to_string(),
                source,
            })?;
    effective_view_from_json(&effective_text, Some(effective_path.to_path_buf()))
        .map_err(TaxonomyReadError::EffectiveInvalid)
}

#[derive(Debug, thiserror::Error)]
pub enum TaxonomyReadError {
    #[error("读取底表分类树失败（{path}）：{source}")]
    BaseRead {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("底表分类树非法：{0}")]
    BaseInvalid(aether_classifier::taxonomy::TaxonomyError),
    #[error("读取生效分类树失败（{path}）：{source}")]
    EffectiveRead {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("生效分类树非法：{0}")]
    EffectiveInvalid(aether_classifier::taxonomy::TaxonomyError),
}

/// 启动分类树：底表 ⊕ 覆盖层 → 生效分类树，并原子导出 effective 文件。
pub fn activate(
    paths: &TaxonomyPaths,
    fs: &dyn FileSystem,
    clock: &dyn Clock,
) -> Result<TaxonomyActivation, TaxonomyStartupError> {
    let base_text =
        fs.read_to_string(paths.base_path())
            .map_err(|source| TaxonomyStartupError::BaseRead {
                path: paths.base_path().display().to_string(),
                source,
            })?;
    let base_view = base_view_from_json(&base_text).map_err(TaxonomyStartupError::BaseInvalid)?;

    fs.create_dir_all(paths.data_dir())
        .map_err(|source| TaxonomyStartupError::DataDir {
            path: paths.data_dir().display().to_string(),
            source,
        })?;

    let mut notices = Vec::new();
    let overlay_path = paths.overlay_path();
    let (overlay, overlay_created) = match fs.read_to_string(&overlay_path) {
        Ok(text) => match parse_overlay(&text, &base_view) {
            Ok(overlay) => (overlay, false),
            Err(reason) => {
                let backup = paths.corrupt_overlay_path(clock.now_ms());
                fs.copy(&overlay_path, &backup).map_err(|source| {
                    TaxonomyStartupError::OverlayBackup {
                        path: backup.display().to_string(),
                        source,
                    }
                })?;
                let empty = OverlayDocument::empty(&base_view.base_version);
                write_overlay(fs, &overlay_path, &empty)?;
                notices.push(HudNotice::taxonomy_overlay_corrupt(
                    &backup.display().to_string(),
                    &reason,
                ));
                (empty, true)
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let empty = OverlayDocument::empty(&base_view.base_version);
            write_overlay(fs, &overlay_path, &empty)?;
            (empty, true)
        }
        Err(source) => {
            return Err(TaxonomyStartupError::OverlayRead {
                path: overlay_path.display().to_string(),
                source,
            })
        }
    };

    let effective_text = merge_effective_document(&base_text, &base_view, &overlay)?;
    let effective_path = paths.effective_path();
    fs.write_atomic(&effective_path, &effective_text)
        .map_err(|source| TaxonomyStartupError::EffectiveWrite {
            path: effective_path.display().to_string(),
            source,
        })?;
    let view = effective_view_from_json(&effective_text, Some(effective_path))
        .map_err(TaxonomyStartupError::EffectiveInvalid)?;

    Ok(TaxonomyActivation {
        view,
        overlay_revision: overlay.revision,
        overlay_created,
        notices,
    })
}

fn write_overlay(
    fs: &dyn FileSystem,
    path: &Path,
    overlay: &OverlayDocument,
) -> Result<(), TaxonomyStartupError> {
    let text = serde_json::to_string_pretty(overlay)
        .map_err(|error| TaxonomyStartupError::Serialize(error.to_string()))?;
    fs.write_atomic(path, &text)
        .map_err(|source| TaxonomyStartupError::OverlayWrite {
            path: path.display().to_string(),
            source,
        })
}

fn parse_overlay(text: &str, base_view: &TaxonomyView) -> Result<OverlayDocument, String> {
    let overlay: OverlayDocument =
        serde_json::from_str(text).map_err(|error| format!("覆盖层 JSON 解析失败：{error}"))?;
    validate_overlay(&overlay, base_view)?;
    Ok(overlay)
}

/// 覆盖层校验（#18 §3.5 第 3 步）：父 L1 存在、id 唯一、同父下名字规范化唯一、字段合法。
fn validate_overlay(overlay: &OverlayDocument, base_view: &TaxonomyView) -> Result<(), String> {
    if overlay.format_version != OVERLAY_FORMAT_VERSION {
        return Err(format!(
            "覆盖层 format_version 应为 {OVERLAY_FORMAT_VERSION}，实际 {}",
            overlay.format_version
        ));
    }
    let normal_l1s = base_view.normal_l1_ids();
    let mut seen_ids: HashSet<&str> = HashSet::new();
    let mut seen_names: HashSet<(String, String)> = HashSet::new();
    for (index, node) in overlay.nodes.iter().enumerate() {
        if node.id.trim().is_empty() {
            return Err(format!("覆盖层 nodes[{index}] 缺少 id"));
        }
        if !node.id.starts_with(SELF_BUILT_L2_PREFIX) {
            return Err(format!(
                "覆盖层 nodes[{index}] 的 id 必须以 {SELF_BUILT_L2_PREFIX} 开头：{}",
                node.id
            ));
        }
        if !seen_ids.insert(node.id.as_str()) {
            return Err(format!("覆盖层自建节点 id 重复：{}", node.id));
        }
        if !normal_l1s.iter().any(|l1| l1 == &node.parent_l1_id) {
            return Err(format!(
                "覆盖层 nodes[{index}] 的父 L1 不存在或不可挂载：{}",
                node.parent_l1_id
            ));
        }
        let name = node.name.trim();
        if !is_valid_new_l2_name(name) {
            return Err(format!(
                "覆盖层 nodes[{index}] 名称须为 2~10 个字符且不含控制字符/斜杠：{:?}",
                node.name
            ));
        }
        let key = (node.parent_l1_id.clone(), normalize_name(name));
        if !seen_names.insert(key) {
            return Err(format!(
                "覆盖层同一父 L1 下名称重复：{} / {}",
                node.parent_l1_id, node.name
            ));
        }
    }
    let mut seen_from: HashSet<&str> = HashSet::new();
    for (index, mapping) in overlay.merged_into.iter().enumerate() {
        if mapping.from.trim().is_empty() || mapping.to.trim().is_empty() {
            return Err(format!("覆盖层 merged_into[{index}] 的 from/to 不能为空"));
        }
        if !seen_from.insert(mapping.from.as_str()) {
            return Err(format!("覆盖层 merged_into 的 from 重复：{}", mapping.from));
        }
    }
    Ok(())
}

fn merge_effective_document(
    base_text: &str,
    base_view: &TaxonomyView,
    overlay: &OverlayDocument,
) -> Result<String, TaxonomyStartupError> {
    let mut document: Value = serde_json::from_str(base_text)
        .map_err(|error| TaxonomyStartupError::Serialize(error.to_string()))?;
    let base_version = base_view.base_version.clone();
    let effective_version = format!("{base_version}+u{}", overlay.revision);

    let mut merged_into: Vec<Value> = overlay
        .merged_into
        .iter()
        .map(|mapping| {
            json!({
                "from": mapping.from,
                "to": mapping.to,
                "at": mapping.at,
            })
        })
        .collect();
    let mut mapped_from: HashSet<String> = overlay
        .merged_into
        .iter()
        .map(|mapping| mapping.from.clone())
        .collect();

    // 官方优先：底表新增节点与自建节点同父 + 规范化名相同时，自建节点并入官方节点
    //（#18 §10）。这里的映射是运行期只读投影、以自建节点的 created_at 作为稳定的 at，
    // 不写盘——持久化 merged 状态、merged_into 与 revision+1 在 #41 的升级对账里落盘。
    let mut official_by_name: HashMap<(String, String), String> = HashMap::new();
    for l1 in base_view.l1s() {
        for child in &l1.children {
            official_by_name.insert(
                (l1.id.clone(), normalize_name(&child.name)),
                child.id.clone(),
            );
        }
    }

    let mut grouped: HashMap<String, Vec<&OverlayNode>> = HashMap::new();
    for node in &overlay.nodes {
        match node.status {
            OverlayNodeStatus::Disabled | OverlayNodeStatus::Merged => continue,
            OverlayNodeStatus::Active => {}
        }
        let collision =
            official_by_name.get(&(node.parent_l1_id.clone(), normalize_name(&node.name)));
        if let Some(official_id) = collision {
            if mapped_from.insert(node.id.clone()) {
                merged_into.push(json!({
                    "from": node.id,
                    "to": official_id,
                    "at": node.created_at,
                }));
            }
            continue;
        }
        grouped
            .entry(node.parent_l1_id.clone())
            .or_default()
            .push(node);
    }

    let taxonomy = document
        .get_mut("taxonomy")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| TaxonomyStartupError::Serialize("底表缺少 taxonomy 数组".to_string()))?;
    for l1_value in taxonomy.iter_mut() {
        let l1_id = l1_value
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let Some(mut nodes) = grouped.remove(&l1_id) else {
            continue;
        };
        nodes.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        });
        let children = l1_value
            .get_mut("children")
            .and_then(Value::as_array_mut)
            .ok_or_else(|| {
                TaxonomyStartupError::Serialize(format!("底表 L1 {l1_id} 缺少 children 数组"))
            })?;
        for node in nodes {
            children.push(json!({
                "id": node.id,
                "name": node.name,
                "l3_preset_tags": node.l3_preset_tags,
            }));
        }
    }

    let object = document
        .as_object_mut()
        .ok_or_else(|| TaxonomyStartupError::Serialize("底表顶层必须是对象".to_string()))?;
    object.insert("version".to_string(), json!(effective_version));
    object.insert("base_version".to_string(), json!(base_version));
    object.insert("merged_into".to_string(), Value::Array(merged_into));

    serde_json::to_string_pretty(&document)
        .map_err(|error| TaxonomyStartupError::Serialize(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::testing::{FakeClock, MemoryFileSystem};

    const BASE: &str = r##"{
      "version": "1.1.0",
      "global_l3_tags": {"format_tags": [
        {"id":"tag_tool","name":"工具","color":"#3B82F6"},
        {"id":"tag_tutorial","name":"教程","color":"#10B981"},
        {"id":"tag_prompt","name":"提示词","color":"#8B5CF6"},
        {"id":"tag_workflow","name":"工作流","color":"#EC4899"},
        {"id":"tag_asset","name":"素材/资源","color":"#F59E0B"}
      ]},
      "taxonomy": [
        {"id":"l1_inbox","name":"00 未分类","icon":"inbox","children":[
          {"id":"l2_manual_review","name":"待人工归档","l3_preset_tags":[]}
        ]},
        {"id":"l1_tech","name":"03 技术与工程","icon":"terminal","children":[
          {"id":"l2_prompt_agent","name":"提示工程与智能体","l3_preset_tags":["Coze"]},
          {"id":"l2_coding","name":"编程与低代码","l3_preset_tags":["Rust"]}
        ]},
        {"id":"l1_biz","name":"04 商业与认知","icon":"trending-up","children":[
          {"id":"l2_insight","name":"行业观察与研报","l3_preset_tags":[]}
        ]}
      ]
    }"##;

    fn paths() -> TaxonomyPaths {
        TaxonomyPaths::new("/data/aether", "/install/taxonomy.json")
    }

    fn fs_with_base() -> MemoryFileSystem {
        MemoryFileSystem::new().with_file(paths().base_path().to_path_buf(), BASE)
    }

    fn overlay_json(nodes: &str, merged: &str, revision: u32) -> String {
        format!(
            r#"{{"format_version":1,"base_version_seen":"1.1.0","revision":{revision},"nodes":[{nodes}],"merged_into":[{merged}]}}"#
        )
    }

    #[test]
    fn missing_overlay_is_created_with_revision_zero_and_effective_tree_exported() {
        let fs = fs_with_base();
        let clock = FakeClock::new(1_761_000_000_000);

        let activation = activate(&paths(), &fs, &clock).unwrap();

        assert!(activation.overlay_created);
        assert_eq!(activation.overlay_revision, 0);
        assert!(activation.notices.is_empty());
        assert_eq!(activation.effective_version(), "1.1.0+u0");
        assert_eq!(activation.base_version(), "1.1.0");

        let overlay_text = fs.read(&paths().overlay_path()).unwrap();
        let overlay: OverlayDocument = serde_json::from_str(&overlay_text).unwrap();
        assert_eq!(overlay.revision, 0);
        assert_eq!(overlay.base_version_seen, "1.1.0");
        assert!(overlay.nodes.is_empty());

        let effective_text = fs.read(&paths().effective_path()).unwrap();
        let effective: Value = serde_json::from_str(&effective_text).unwrap();
        assert_eq!(effective["version"], "1.1.0+u0");
        assert_eq!(effective["base_version"], "1.1.0");
        assert_eq!(effective["taxonomy"].as_array().unwrap().len(), 3);
        assert_eq!(activation.view.normal_l2_ids().len(), 3);
        assert_eq!(
            activation.view.effective_source,
            aether_classifier::taxonomy::EffectiveSource::File
        );
    }

    #[test]
    fn self_built_nodes_are_appended_by_created_at_disabled_and_merged_excluded() {
        let fs = fs_with_base().with_file(
            paths().overlay_path(),
            overlay_json(
                r#"{"id":"l2u_b","parent_l1_id":"l1_tech","name":"交易脚本","l3_preset_tags":["Polymarket"],"status":"active","created_at":200},
                   {"id":"l2u_a","parent_l1_id":"l1_tech","name":"短视频脚本","status":"active","created_at":100},
                   {"id":"l2u_old","parent_l1_id":"l1_tech","name":"旧节点","status":"disabled","created_at":50},
                   {"id":"l2u_merged","parent_l1_id":"l1_biz","name":"已合并","status":"merged","created_at":60}"#,
                r#"{"from":"l2u_merged","to":"l2_insight","at":1761000000000}"#,
                3,
            ),
        );
        let clock = FakeClock::new(1_761_000_000_000);

        let activation = activate(&paths(), &fs, &clock).unwrap();
        let view = &activation.view;

        assert_eq!(activation.effective_version(), "1.1.0+u3");
        assert_eq!(activation.overlay_revision, 3);
        let tech_ids: Vec<&str> = view
            .l1s()
            .iter()
            .find(|l1| l1.id == "l1_tech")
            .unwrap()
            .children
            .iter()
            .map(|l2| l2.id.as_str())
            .collect();
        assert_eq!(
            tech_ids,
            vec!["l2_prompt_agent", "l2_coding", "l2u_a", "l2u_b"]
        );
        assert!(!view.all_l2_ids().iter().any(|id| id == "l2u_old"));
        assert!(!view.all_l2_ids().iter().any(|id| id == "l2u_merged"));
        assert_eq!(
            view.remap_l2_id("l2u_merged").as_deref(),
            Some("l2_insight")
        );
        assert_eq!(view.l2_name("l2u_b"), Some("交易脚本"));
        assert_eq!(
            view.l2s().find(|l2| l2.id == "l2u_b").unwrap().preset_tags,
            vec!["Polymarket"]
        );

        let effective_text = fs.read(&paths().effective_path()).unwrap();
        let effective: Value = serde_json::from_str(&effective_text).unwrap();
        let merged = effective["merged_into"].as_array().unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0]["from"], "l2u_merged");
        assert_eq!(merged[0]["to"], "l2_insight");
    }

    #[test]
    fn official_nodes_keep_base_order() {
        let fs = fs_with_base();
        let clock = FakeClock::new(1_761_000_000_000);
        let activation = activate(&paths(), &fs, &clock).unwrap();
        let tech = activation
            .view
            .l1s()
            .iter()
            .find(|l1| l1.id == "l1_tech")
            .unwrap();
        let ids: Vec<&str> = tech.children.iter().map(|l2| l2.id.as_str()).collect();
        assert_eq!(ids, vec!["l2_prompt_agent", "l2_coding"]);
        let l1_ids: Vec<&str> = activation
            .view
            .l1s()
            .iter()
            .map(|l1| l1.id.as_str())
            .collect();
        assert_eq!(l1_ids, vec!["l1_inbox", "l1_tech", "l1_biz"]);
    }

    #[test]
    fn official_collision_merges_self_built_into_official_node() {
        let fs = fs_with_base().with_file(
            paths().overlay_path(),
            overlay_json(
                r#"{"id":"l2u_dup","parent_l1_id":"l1_tech","name":" 编程与低代码 ","status":"active","created_at":100}"#,
                "",
                2,
            ),
        );
        let clock = FakeClock::new(1_761_000_000_000);

        let activation = activate(&paths(), &fs, &clock).unwrap();

        assert!(!activation
            .view
            .all_l2_ids()
            .iter()
            .any(|id| id == "l2u_dup"));
        assert_eq!(
            activation.view.remap_l2_id("l2u_dup").as_deref(),
            Some("l2_coding")
        );
        let effective: Value =
            serde_json::from_str(&fs.read(&paths().effective_path()).unwrap()).unwrap();
        assert_eq!(effective["merged_into"][0]["from"], "l2u_dup");
        assert_eq!(effective["merged_into"][0]["to"], "l2_coding");
    }

    #[test]
    fn corrupt_overlay_is_backed_up_notice_emitted_and_base_tree_used() {
        let corrupt = "{\"format_version\": 1, not json";
        let fs = fs_with_base().with_file(paths().overlay_path(), corrupt);
        let clock = FakeClock::new(1_761_000_000_000);

        let activation = activate(&paths(), &fs, &clock).unwrap();

        assert_eq!(activation.notices.len(), 1);
        let notice = &activation.notices[0];
        assert_eq!(
            notice.code,
            crate::notice::HudNoticeCode::TaxonomyOverlayCorrupt
        );
        let backup_path = paths().corrupt_overlay_path(1_761_000_000_000);
        assert!(notice.detail.contains(&backup_path.display().to_string()));
        assert_eq!(fs.read(&backup_path).as_deref(), Some(corrupt));
        assert_eq!(activation.effective_version(), "1.1.0+u0");
        assert!(activation.view.self_built_l2_ids().is_empty());
        let overlay_text = fs.read(&paths().overlay_path()).unwrap();
        let overlay: OverlayDocument = serde_json::from_str(&overlay_text).unwrap();
        assert_eq!(overlay.revision, 0);
        assert!(overlay.nodes.is_empty());
    }

    #[test]
    fn semantically_invalid_overlay_falls_back_like_a_corrupt_one() {
        let fs = fs_with_base().with_file(
            paths().overlay_path(),
            overlay_json(
                r#"{"id":"l2u_orphan","parent_l1_id":"l1_missing","name":"孤儿节点","created_at":100}"#,
                "",
                1,
            ),
        );
        let clock = FakeClock::new(1_761_000_000_000);

        let activation = activate(&paths(), &fs, &clock).unwrap();

        assert_eq!(activation.notices.len(), 1);
        let backup = fs
            .read(&paths().corrupt_overlay_path(1_761_000_000_000))
            .unwrap();
        assert!(backup.contains("l2u_orphan"));
        assert_eq!(activation.effective_version(), "1.1.0+u0");
    }

    #[test]
    fn invalid_overlay_name_length_is_rejected_as_corrupt() {
        let fs = fs_with_base().with_file(
            paths().overlay_path(),
            overlay_json(
                r#"{"id":"l2u_short","parent_l1_id":"l1_tech","name":"短","created_at":100}"#,
                "",
                1,
            ),
        );
        let clock = FakeClock::new(1_761_000_000_000);
        let activation = activate(&paths(), &fs, &clock).unwrap();
        assert_eq!(activation.notices.len(), 1);
    }

    #[test]
    fn exported_view_uses_effective_file_and_falls_back_to_base() {
        let fs = fs_with_base();
        let base_only =
            load_exported_view(&fs, paths().base_path(), Some(&paths().effective_path())).unwrap();
        assert_eq!(base_only.effective_version, "1.1.0");
        assert_eq!(
            base_only.effective_source,
            aether_classifier::taxonomy::EffectiveSource::BaseFallback
        );

        let effective = BASE
            .replace(r#""version": "1.1.0","#, r#""version": "1.1.0+u4", "base_version": "1.1.0", "merged_into": [],"#)
            .replace(
                r#"{"id":"l2_coding","name":"编程与低代码","l3_preset_tags":["Rust"]}"#,
                r#"{"id":"l2_coding","name":"编程与低代码","l3_preset_tags":["Rust"]},{"id":"l2u_exported","name":"评测自建"}"#,
            );
        let fs = fs.with_file(paths().effective_path(), effective);
        let view =
            load_exported_view(&fs, paths().base_path(), Some(&paths().effective_path())).unwrap();
        assert_eq!(view.effective_version, "1.1.0+u4");
        assert_eq!(
            view.effective_source,
            aether_classifier::taxonomy::EffectiveSource::File
        );
        assert!(view.is_normal_l2("l2u_exported"));
    }

    #[test]
    fn exported_view_rejects_an_invalid_effective_file() {
        let fs = fs_with_base().with_file(paths().effective_path(), "{}");
        let error = load_exported_view(&fs, paths().base_path(), Some(&paths().effective_path()))
            .unwrap_err();
        assert!(matches!(error, TaxonomyReadError::EffectiveInvalid(_)));
    }

    #[test]
    fn base_that_is_invalid_is_fatal_not_a_silent_fallback() {
        let fs = MemoryFileSystem::new().with_file(paths().base_path().to_path_buf(), "{}");
        let clock = FakeClock::new(1);
        let error = activate(&paths(), &fs, &clock).unwrap_err();
        assert!(matches!(error, TaxonomyStartupError::BaseInvalid(_)));
    }
}
