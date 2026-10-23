//! 收件箱视图数据（#28）：管线视图的 keyset 分页行。
//!
//! 收件箱 = pipeline_status ∈ (pending, processing, failed) 的条目，按 created_at
//! 倒序、50 条/页；游标用最后一行 (created_at, id)，翻页期间新入库不会重复或丢行。

use serde::{Deserialize, Serialize};

/// 收件箱固定页大小（#24 §13：50 条/页 keyset 分页）。
pub const INBOX_PAGE_SIZE: usize = 50;

/// 收件箱 keyset 游标：上一页最后一行的 (created_at, id)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxCursor {
    pub created_at: i64,
    pub id: i64,
}

/// 收件箱行：只给前端列表需要的字段（显示名从生效分类树解析，不冗余）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxEntry {
    pub id: i64,
    pub uid: String,
    pub title: String,
    pub url: Option<String>,
    pub media_type: String,
    pub pipeline_status: String,
    pub failure_reason: Option<String>,
    pub created_at: i64,
    pub last_seen_at: i64,
    pub l1_id: String,
    pub l2_id: String,
}

/// 一页收件箱：next_cursor = None 表示没有更多。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxPage {
    pub entries: Vec<InboxEntry>,
    pub next_cursor: Option<InboxCursor>,
}
