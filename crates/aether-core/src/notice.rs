//! HUD 提示条（#13 提示条机制的早期消费方；#27 用到覆盖层损坏与核心库未启动）。
//!
//! 领域层只产出 code + 可读 detail；展示时长与样式属于前端（5 秒、不抢焦点）。

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HudNoticeCode {
    /// 覆盖层损坏：已备份为 corrupt 快照，按底表启动。
    TaxonomyOverlayCorrupt,
    /// 核心库未能启动（分类树/数据库不可用）：HUD 只剩占位球。
    CoreUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HudNotice {
    pub code: HudNoticeCode,
    pub detail: String,
}

impl HudNotice {
    pub fn taxonomy_overlay_corrupt(backup_path: &str, reason: &str) -> Self {
        Self {
            code: HudNoticeCode::TaxonomyOverlayCorrupt,
            detail: format!(
                "分类树覆盖层损坏，已备份为 {backup_path} 并按底表启动、未合并覆盖层（{reason}）"
            ),
        }
    }

    pub fn core_unavailable(reason: &str) -> Self {
        Self {
            code: HudNoticeCode::CoreUnavailable,
            detail: format!("核心库未启动，分类树与球列不可用：{reason}"),
        }
    }
}
