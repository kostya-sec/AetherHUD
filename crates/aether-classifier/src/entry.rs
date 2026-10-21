//! 分类输入类型（与 #10 entries 表及 #19 §12 的接口定义对齐）。

use serde::{Deserialize, Serialize};

/// 送进分类器的最小条目视图。
///
/// 与 entries 表的映射由调用方完成：title、url、media_type、body_md；
/// 视频元数据来自 parser（#2），缺失时整体为 None。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntryForClassify {
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default = "default_media_type")]
    pub media_type: String,
    #[serde(default)]
    pub body_excerpt: Option<String>,
    #[serde(default)]
    pub video_meta: Option<VideoMeta>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoMeta {
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub description_excerpt: Option<String>,
}

fn default_media_type() -> String {
    "link".to_string()
}

impl EntryForClassify {
    pub fn new(
        title: impl Into<String>,
        url: Option<String>,
        media_type: impl Into<String>,
        body_excerpt: Option<String>,
    ) -> Self {
        Self {
            title: title.into(),
            url,
            media_type: media_type.into(),
            body_excerpt,
            video_meta: None,
        }
    }
}
