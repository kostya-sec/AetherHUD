//! LLM 供应商配置（#45 前置）：单一活动供应商 + 预设 + 消息类型。
//!
//! 2026-10-23 用户澄清后定下的三条边界：
//! 1. **单一活动供应商**：同一时间只有一套配置生效；切换预设要重填 Key（Key 仍存
//!    Keychain account `classifier.api_key`），不做多套配置列表。预设只提供默认
//!    base URL / model / message type，用户可直接改；「自定义」要求自己填。
//! 2. **非秘密**：base URL / model / 消息类型是普通 settings（value_json），密钥本
//!    体永远只在系统钥匙串，库里最多留 `keychain://aetherhud/<account>` 引用。
//! 3. **坏值不阻塞界面**：settings 里存了无法解析的值（旧版本、手工改坏）时逐键回
//!    落到预设默认值，设置页照常打开。
//!
//! 本模块不读系统时钟、不连网络：保存时间由调用方传入，映射结果交给
//! `aether-classifier` 的 provider 调用方使用。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::storage::{Database, StorageError};

/// settings 键：活动供应商预设（`bailian` / `openai` / `deepseek` / `custom`）。
pub const PROVIDER_SETTING: &str = "classifier.provider";
/// settings 键：OpenAI 兼容 Base URL（分类验证与分类 worker 共用）。
pub const BASE_URL_SETTING: &str = "classifier.base_url";
/// settings 键：模型 ID。
pub const MODEL_SETTING: &str = "classifier.model";
/// settings 键：消息类型（`chat_completions` / `responses`）。
pub const MESSAGE_TYPE_SETTING: &str = "classifier.message_type";

/// 百炼（DashScope）OpenAI 兼容端点（v1 默认供应商）。
pub const BAILIAN_BASE_URL: &str = "https://dashscope.aliyuncs.com/compatible-mode/v1";
/// OpenAI 默认端点。
pub const OPENAI_BASE_URL: &str = "https://api.openai.com/v1";
/// DeepSeek 默认端点（OpenAI 兼容）。
pub const DEEPSEEK_BASE_URL: &str = "https://api.deepseek.com/v1";

/// base URL 长度上限：足够放下带路径/网关前缀的端点，又挡住误粘贴的大段文本。
pub const MAX_BASE_URL_CHARS: usize = 2048;
/// 模型 ID 长度上限（够长于现有任何模型名）。
pub const MAX_MODEL_CHARS: usize = 256;

/// 消息类型（Message Type）：与分类路径共用同一个枚举，避免两处定义漂移。
///
/// 设置层需要 `as_str` / `parse`，由 aether-classifier 提供（它是唯一分类路径）。
pub use aether_classifier::provider::MessageType;

/// 供应商预设：只提供默认值，不锁定用户改动。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProviderPreset {
    /// 阿里百炼 DashScope（v1 默认）。
    #[default]
    #[serde(rename = "bailian")]
    Bailian,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "deepseek")]
    DeepSeek,
    /// 自定义 OpenAI 兼容端点：base URL / model 必须由用户填写。
    #[serde(rename = "custom")]
    Custom,
}

impl ProviderPreset {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bailian => "bailian",
            Self::OpenAi => "openai",
            Self::DeepSeek => "deepseek",
            Self::Custom => "custom",
        }
    }

    /// 宽松解析：不认识的值返回 None（调用方回落百炼）。
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "bailian" | "dashscope" | "dashscope-compatible" => Some(Self::Bailian),
            "openai" => Some(Self::OpenAi),
            "deepseek" => Some(Self::DeepSeek),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }

    pub const fn default_base_url(self) -> &'static str {
        match self {
            Self::Bailian => BAILIAN_BASE_URL,
            Self::OpenAi => OPENAI_BASE_URL,
            Self::DeepSeek => DEEPSEEK_BASE_URL,
            Self::Custom => "",
        }
    }

    pub const fn default_model(self) -> &'static str {
        match self {
            Self::Bailian => "qwen-flash",
            Self::OpenAi => "gpt-4o-mini",
            Self::DeepSeek => "deepseek-chat",
            Self::Custom => "",
        }
    }

    pub const fn default_message_type(self) -> MessageType {
        match self {
            // v1 三个内置预设都走 Chat Completions；Responses 由用户显式切换。
            Self::Bailian | Self::OpenAi | Self::DeepSeek | Self::Custom => {
                MessageType::ChatCompletions
            }
        }
    }

    /// 界面与错误串里的供应商名（不写实现细节）。
    pub const fn label(self) -> &'static str {
        // 展示名与前端 PROVIDER_PRESETS 保持一致（跨层漂移由两侧测试分别锁定）。
        match self {
            Self::Bailian => "阿里百炼（DashScope）",
            Self::OpenAi => "OpenAI",
            Self::DeepSeek => "DeepSeek",
            Self::Custom => "自定义（OpenAI 兼容）",
        }
    }
}

/// 活动供应商配置：预设 + 可改的 base URL / model / 消息类型（前端同形，camelCase）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProfile {
    pub preset: ProviderPreset,
    pub base_url: String,
    pub model: String,
    pub message_type: MessageType,
}

impl Default for ProviderProfile {
    /// 默认 = 百炼的预设默认值（与 #15 规格一致）。
    fn default() -> Self {
        Self::from_preset(ProviderPreset::Bailian)
    }
}

impl ProviderProfile {
    /// 预设的初始草稿（只填默认值，不代表已保存）。
    pub fn from_preset(preset: ProviderPreset) -> Self {
        Self {
            preset,
            base_url: preset.default_base_url().to_string(),
            model: preset.default_model().to_string(),
            message_type: preset.default_message_type(),
        }
    }

    /// 保存前的规范化：去首尾空白、去掉 base URL 末尾的 `/`。
    pub fn normalized(&self) -> Self {
        Self {
            preset: self.preset,
            base_url: self.base_url.trim().trim_end_matches('/').to_string(),
            model: self.model.trim().to_string(),
            message_type: self.message_type,
        }
    }

    /// 保存前校验：base URL 必须是 http/https 且带主机，model 非空。
    ///
    /// 错误串直接展示给用户，所以只写需要改什么，不写内部实现。
    pub fn validate(&self) -> Result<(), ProviderProfileError> {
        let base_url = self.base_url.trim().trim_end_matches('/');
        if base_url.is_empty() {
            return Err(ProviderProfileError::EmptyBaseUrl);
        }
        if base_url.chars().count() > MAX_BASE_URL_CHARS {
            return Err(ProviderProfileError::BaseUrlTooLong);
        }
        if !is_valid_base_url(base_url) {
            return Err(ProviderProfileError::InvalidBaseUrl(base_url.to_string()));
        }
        let model = self.model.trim();
        if model.is_empty() {
            return Err(ProviderProfileError::EmptyModel);
        }
        if model.chars().count() > MAX_MODEL_CHARS {
            return Err(ProviderProfileError::ModelTooLong);
        }
        Ok(())
    }

    /// 校验 + 规范化后的一次保存：逐键写 settings（value_json）。
    ///
    /// 四个键属于同一套活动配置，调用方（壳）负责在同一个命令里保存；设置表没有
    /// 多键事务，写到一半失败时下次读取会按「坏值回落」兜底，不会让界面崩溃。
    pub fn save(
        database: &Database,
        profile: &ProviderProfile,
        now_ms: i64,
    ) -> Result<ProviderProfile, ProviderProfileError> {
        let profile = profile.normalized();
        profile.validate()?;
        database.setting_set(
            PROVIDER_SETTING,
            &profile.preset.as_str().to_string(),
            now_ms,
        )?;
        database.setting_set(BASE_URL_SETTING, &profile.base_url, now_ms)?;
        database.setting_set(MODEL_SETTING, &profile.model, now_ms)?;
        database.setting_set(
            MESSAGE_TYPE_SETTING,
            &profile.message_type.as_str().to_string(),
            now_ms,
        )?;
        Ok(profile)
    }

    /// 读取活动配置；缺失或坏值逐键回落到预设默认值（设置页永远能打开）。
    ///
    /// 读设置出错也按「没配过」处理：配置层不阻塞 UI；写失败由 [`Self::save`] 报错。
    pub fn load(database: &Database) -> Self {
        let preset = read_setting(database, PROVIDER_SETTING)
            .and_then(|value| ProviderPreset::parse(&value))
            .unwrap_or_default();

        let base_url = match read_setting(database, BASE_URL_SETTING) {
            Some(raw) => {
                let trimmed = raw.trim().trim_end_matches('/');
                if is_valid_base_url(trimmed) {
                    trimmed.to_string()
                } else {
                    preset.default_base_url().to_string()
                }
            }
            None => preset.default_base_url().to_string(),
        };

        let model = match read_setting(database, MODEL_SETTING) {
            Some(raw) if is_valid_model(&raw) => raw.trim().to_string(),
            _ => preset.default_model().to_string(),
        };

        let message_type = read_setting(database, MESSAGE_TYPE_SETTING)
            .and_then(|value| MessageType::parse(&value))
            .unwrap_or_else(|| preset.default_message_type());

        Self {
            preset,
            base_url,
            model,
            message_type,
        }
    }

    /// core → aether-classifier 的接缝：worker / eval 都从这里拿 provider 配置。
    ///
    /// 分类是「短文本 → 多选映射」：temperature 0、max_tokens 400、30s 超时；
    /// 代理归「网络」分区（#45）后由调用方覆盖，这里不读环境变量。extra_body 留空，
    /// 保持 aether-classifier 对百炼的既有行为不变。
    pub fn to_classifier_config(
        &self,
        api_key: String,
    ) -> aether_classifier::provider::ProviderConfig {
        let profile = self.normalized();
        aether_classifier::provider::ProviderConfig {
            provider: profile.preset.as_str().to_string(),
            base_url: profile.base_url,
            model: profile.model,
            api_key,
            message_type: profile.message_type,
            temperature: 0.0,
            max_tokens: 400,
            request_timeout: Duration::from_secs(30),
            proxy: None,
            extra_body: Default::default(),
        }
    }
}

/// 供应商配置错误：全部可读，界面直接展示。
#[derive(Debug, thiserror::Error)]
pub enum ProviderProfileError {
    #[error("Base URL 不能为空（自定义供应商需填写完整端点，例如 https://api.example.com/v1）")]
    EmptyBaseUrl,
    #[error("Base URL 必须以 http:// 或 https:// 开头：{0}")]
    InvalidBaseUrl(String),
    #[error("Base URL 过长（最多 {MAX_BASE_URL_CHARS} 个字符）")]
    BaseUrlTooLong,
    #[error("模型 ID 不能为空（例如 qwen-flash / gpt-4o-mini / deepseek-chat）")]
    EmptyModel,
    #[error("模型 ID 过长（最多 {MAX_MODEL_CHARS} 个字符）")]
    ModelTooLong,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

fn read_setting(database: &Database, key: &str) -> Option<String> {
    database.setting_get::<String>(key).ok().flatten()
}

fn is_valid_base_url(value: &str) -> bool {
    let Ok(parsed) = Url::parse(value) else {
        return false;
    };
    matches!(parsed.scheme(), "http" | "https") && parsed.host_str().is_some()
}

fn is_valid_model(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty() && trimmed.chars().count() <= MAX_MODEL_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_database() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn default_profile_is_bailian_chat_completions() {
        let profile = ProviderProfile::default();
        assert_eq!(profile.preset, ProviderPreset::Bailian);
        assert_eq!(profile.base_url, BAILIAN_BASE_URL);
        assert_eq!(profile.model, "qwen-flash");
        assert_eq!(profile.message_type, MessageType::ChatCompletions);
    }

    #[test]
    fn each_preset_ships_its_documented_defaults() {
        let bailian = ProviderProfile::from_preset(ProviderPreset::Bailian);
        assert_eq!(
            bailian.base_url,
            "https://dashscope.aliyuncs.com/compatible-mode/v1"
        );
        assert_eq!(bailian.model, "qwen-flash");

        let openai = ProviderProfile::from_preset(ProviderPreset::OpenAi);
        assert_eq!(openai.base_url, "https://api.openai.com/v1");
        assert_eq!(openai.model, "gpt-4o-mini");

        let deepseek = ProviderProfile::from_preset(ProviderPreset::DeepSeek);
        assert_eq!(deepseek.base_url, "https://api.deepseek.com/v1");
        assert_eq!(deepseek.model, "deepseek-chat");

        let custom = ProviderProfile::from_preset(ProviderPreset::Custom);
        assert!(custom.base_url.is_empty());
        assert!(custom.model.is_empty());

        for preset in [
            ProviderPreset::Bailian,
            ProviderPreset::OpenAi,
            ProviderPreset::DeepSeek,
            ProviderPreset::Custom,
        ] {
            assert_eq!(
                ProviderProfile::from_preset(preset).message_type,
                MessageType::ChatCompletions,
                "内置预设默认走 Chat Completions"
            );
        }
    }

    #[test]
    fn invalid_base_url_and_model_are_rejected_with_readable_errors() {
        let empty_base = ProviderProfile {
            preset: ProviderPreset::Custom,
            base_url: "   ".to_string(),
            model: "m".to_string(),
            message_type: MessageType::ChatCompletions,
        };
        let error = empty_base.validate().unwrap_err().to_string();
        assert!(error.contains("Base URL 不能为空"), "{error}");

        let not_http = ProviderProfile {
            preset: ProviderPreset::Custom,
            base_url: "api.example.com/v1".to_string(),
            model: "m".to_string(),
            message_type: MessageType::ChatCompletions,
        };
        let error = not_http.validate().unwrap_err().to_string();
        assert!(error.contains("http:// 或 https://"), "{error}");
        assert!(
            error.contains("api.example.com"),
            "错误串要带上填错的值：{error}"
        );

        let no_host = ProviderProfile {
            preset: ProviderPreset::Custom,
            base_url: "https://".to_string(),
            model: "m".to_string(),
            message_type: MessageType::ChatCompletions,
        };
        assert!(no_host.validate().is_err());

        let empty_model = ProviderProfile {
            preset: ProviderPreset::OpenAi,
            base_url: "https://api.openai.com/v1".to_string(),
            model: "  ".to_string(),
            message_type: MessageType::ChatCompletions,
        };
        let error = empty_model.validate().unwrap_err().to_string();
        assert!(error.contains("模型 ID 不能为空"), "{error}");
    }

    #[test]
    fn save_and_load_round_trip_including_custom_provider() {
        let database = fresh_database();
        let custom = ProviderProfile {
            preset: ProviderPreset::Custom,
            base_url: " https://gateway.example.com/openai/v1/ ".to_string(),
            model: " my-model ".to_string(),
            message_type: MessageType::Responses,
        };

        let saved = ProviderProfile::save(&database, &custom, 1_000).unwrap();

        assert_eq!(saved.base_url, "https://gateway.example.com/openai/v1");
        assert_eq!(saved.model, "my-model");
        let loaded = ProviderProfile::load(&database);
        assert_eq!(loaded, saved);

        let snapshot = database.settings_snapshot();
        assert!(snapshot
            .iter()
            .any(|(key, value)| { key == PROVIDER_SETTING && value.contains("custom") }));
        assert!(snapshot.iter().any(|(key, _)| key == BASE_URL_SETTING));
        assert!(snapshot
            .iter()
            .any(|(key, value)| key == MESSAGE_TYPE_SETTING && value.contains("responses")));
        assert_eq!(snapshot.len(), 4, "四个配置键各一行：{snapshot:?}");
    }

    #[test]
    fn bad_stored_values_fall_back_to_preset_defaults() {
        let database = fresh_database();
        database
            .setting_set(PROVIDER_SETTING, &"anthropic".to_string(), 0)
            .unwrap();
        database
            .setting_set(BASE_URL_SETTING, &"not a url".to_string(), 0)
            .unwrap();
        database
            .setting_set(MODEL_SETTING, &"   ".to_string(), 0)
            .unwrap();
        database
            .setting_set(MESSAGE_TYPE_SETTING, &"grpc".to_string(), 0)
            .unwrap();

        let loaded = ProviderProfile::load(&database);

        assert_eq!(loaded, ProviderProfile::default());
    }

    #[test]
    fn custom_preset_without_a_saved_base_url_loads_empty_instead_of_bailian() {
        let database = fresh_database();
        database
            .setting_set(PROVIDER_SETTING, &"custom".to_string(), 0)
            .unwrap();

        let loaded = ProviderProfile::load(&database);

        assert_eq!(loaded.preset, ProviderPreset::Custom);
        assert!(loaded.base_url.is_empty());
        assert!(loaded.model.is_empty());
        assert_eq!(loaded.message_type, MessageType::ChatCompletions);
    }

    #[test]
    fn missing_settings_load_the_bailian_default() {
        let database = fresh_database();
        assert_eq!(ProviderProfile::load(&database), ProviderProfile::default());
    }

    #[test]
    fn mapping_to_the_classifier_config_keeps_the_classifier_defaults() {
        let profile = ProviderProfile {
            preset: ProviderPreset::OpenAi,
            base_url: "https://api.openai.com/v1/".to_string(),
            model: "gpt-4o-mini".to_string(),
            message_type: MessageType::Responses,
        };

        let config = profile.to_classifier_config("sk-test".to_string());

        assert_eq!(config.provider, "openai");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(
            config.message_type,
            aether_classifier::provider::MessageType::Responses
        );
        assert_eq!(config.temperature, 0.0);
        assert_eq!(config.max_tokens, 400);
        assert_eq!(config.request_timeout, Duration::from_secs(30));
        assert!(config.proxy.is_none());
        assert!(config.extra_body.is_empty());
    }
}
