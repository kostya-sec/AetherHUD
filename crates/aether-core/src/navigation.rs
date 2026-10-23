//! HUD 导航数据：球列 5 个 L1 与 L2 面板内容（#27 / #17 §5.4 / §5.5）。
//!
//! 只从生效分类树派生，不重复分类树逻辑；前端拿到的是扁平 DTO，
//! 「全部子类」等纯 UI 条目由前端补充。

use aether_classifier::taxonomy::TaxonomyView;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudNavigation {
    pub base_version: String,
    pub effective_version: String,
    pub l1s: Vec<HudL1>,
    pub format_tags: Vec<HudFormatTag>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudL1 {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
    pub l2s: Vec<HudL2>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudL2 {
    pub id: String,
    pub name: String,
    pub self_built: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudFormatTag {
    pub id: String,
    pub name: String,
    pub color: Option<String>,
}

pub fn build(view: &TaxonomyView) -> HudNavigation {
    HudNavigation {
        base_version: view.base_version.clone(),
        effective_version: view.effective_version.clone(),
        l1s: view
            .l1s()
            .iter()
            .map(|l1| HudL1 {
                id: l1.id.clone(),
                name: l1.name.clone(),
                icon: l1.icon.clone(),
                l2s: l1
                    .children
                    .iter()
                    .map(|l2| HudL2 {
                        id: l2.id.clone(),
                        name: l2.name.clone(),
                        self_built: l2.self_built,
                    })
                    .collect(),
            })
            .collect(),
        format_tags: view
            .format_tags()
            .iter()
            .map(|tag| HudFormatTag {
                id: tag.id.clone(),
                name: tag.name.clone(),
                color: tag.color.clone(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aether_classifier::taxonomy::{base_view_from_json, effective_view_from_json};

    const BASE: &str = r##"{
      "version": "1.1.0",
      "global_l3_tags": {"format_tags": [
        {"id":"tag_tool","name":"工具","color":"#3B82F6"},
        {"id":"tag_tutorial","name":"教程"},
        {"id":"tag_prompt","name":"提示词"},
        {"id":"tag_workflow","name":"工作流"},
        {"id":"tag_asset","name":"素材/资源"}
      ]},
      "taxonomy": [
        {"id":"l1_inbox","name":"00 未分类","icon":"inbox","children":[{"id":"l2_manual_review","name":"待人工归档"}]},
        {"id":"l1_content","name":"01 内容与表达","icon":"palette","children":[{"id":"l2_writing","name":"文本与写作"}]},
        {"id":"l1_productivity","name":"02 效率与工作流","icon":"zap","children":[{"id":"l2_km","name":"信息与知识管理"}]},
        {"id":"l1_tech","name":"03 技术与工程","icon":"terminal","children":[{"id":"l2_coding","name":"编程与低代码"}]},
        {"id":"l1_biz","name":"04 商业与认知","icon":"trending-up","children":[{"id":"l2_insight","name":"行业观察与研报"}]}
      ]
    }"##;

    #[test]
    fn ball_order_comes_from_the_effective_tree_and_keeps_inbox_first() {
        let view = base_view_from_json(BASE).unwrap();
        let navigation = build(&view);
        let ids: Vec<&str> = navigation.l1s.iter().map(|l1| l1.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "l1_inbox",
                "l1_content",
                "l1_productivity",
                "l1_tech",
                "l1_biz"
            ]
        );
        assert_eq!(navigation.l1s.len(), 5);
        assert_eq!(navigation.l1s[0].name, "00 未分类");
        assert_eq!(navigation.l1s[3].icon.as_deref(), Some("terminal"));
        assert_eq!(
            navigation
                .l1s
                .iter()
                .find(|l1| l1.id == "l1_tech")
                .unwrap()
                .l2s[0]
                .name,
            "编程与低代码"
        );
        assert_eq!(navigation.format_tags.len(), 5);
        assert_eq!(navigation.format_tags[0].color.as_deref(), Some("#3B82F6"));
        assert_eq!(navigation.format_tags[1].color, None);
    }

    #[test]
    fn self_built_l2_appears_at_the_end_of_its_parent() {
        let effective = BASE.replace(
            r#"{"id":"l2_coding","name":"编程与低代码"}"#,
            r#"{"id":"l2_coding","name":"编程与低代码"},{"id":"l2u_x","name":"交易脚本"}"#,
        );
        let effective = effective.replace(
            r#""version": "1.1.0","#,
            r#""version": "1.1.0+u2","base_version": "1.1.0","merged_into": [],"#,
        );
        let view = effective_view_from_json(&effective, None).unwrap();
        let navigation = build(&view);
        assert_eq!(navigation.effective_version, "1.1.0+u2");
        assert_eq!(navigation.base_version, "1.1.0");
        let tech = navigation.l1s.iter().find(|l1| l1.id == "l1_tech").unwrap();
        let ids: Vec<&str> = tech.l2s.iter().map(|l2| l2.id.as_str()).collect();
        assert_eq!(ids, vec!["l2_coding", "l2u_x"]);
        assert!(tech.l2s[1].self_built);
        assert!(!tech.l2s[0].self_built);
    }
}
