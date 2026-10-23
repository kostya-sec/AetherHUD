//! OpenAI 兼容 provider：Chat Completions 与 Responses 两种消息格式（默认阿里百炼
//! DashScope，见 #15 / #45 前置）。
//!
//! 单次调用只做一次 HTTP；重试由 [`crate::engine::ClassificationEngine`] 按 #11 策略驱动。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::prompt::Message;
use crate::retry::ErrorClass;

/// 消息格式（API 协议）：决定请求体形状与响应解析路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// POST {base}/chat/completions，body.messages[]，读 choices[0].message.content。
    #[default]
    ChatCompletions,
    /// POST {base}/responses，body.input[]，读 output_text 或 output[].content[]。
    Responses,
}

impl MessageType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChatCompletions => "chat_completions",
            Self::Responses => "responses",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// provider 名称（报告参数；"dashscope" 时附加 enable_thinking=false）。
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    /// 消息格式（默认 Chat Completions，向后兼容既有调用点）。
    pub message_type: MessageType,
    pub temperature: f64,
    pub max_tokens: u32,
    pub request_timeout: Duration,
    pub proxy: Option<String>,
    pub extra_body: Map<String, Value>,
}

impl ProviderConfig {
    pub fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        match self.message_type {
            MessageType::ChatCompletions => {
                if base.ends_with("/chat/completions") {
                    base.to_string()
                } else {
                    format!("{base}/chat/completions")
                }
            }
            MessageType::Responses => {
                if base.ends_with("/responses") {
                    base.to_string()
                } else {
                    format!("{base}/responses")
                }
            }
        }
    }

    pub fn is_dashscope(&self) -> bool {
        self.provider.eq_ignore_ascii_case("dashscope")
            || self.provider.eq_ignore_ascii_case("bailian")
            || self.provider.eq_ignore_ascii_case("dashscope-compatible")
            || self.base_url.contains("dashscope.aliyuncs.com")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
}

impl Usage {
    pub fn add(&mut self, other: &Usage) {
        self.prompt_tokens += other.prompt_tokens;
        self.completion_tokens += other.completion_tokens;
    }

    pub fn total(&self) -> u64 {
        self.prompt_tokens + self.completion_tokens
    }
}

#[derive(Debug, Clone)]
pub struct ChatResponse {
    pub content: String,
    pub usage: Usage,
    pub finish_reason: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProviderError {
    pub class: ErrorClass,
    pub http_status: Option<u16>,
    pub message: String,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.http_status {
            Some(status) => write!(f, "HTTP {status}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for ProviderError {}

impl ProviderError {
    pub fn terminal(message: impl Into<String>) -> Self {
        Self {
            class: ErrorClass::Terminal,
            http_status: None,
            message: message.into(),
        }
    }

    pub fn retryable(message: impl Into<String>) -> Self {
        Self {
            class: ErrorClass::Retryable,
            http_status: None,
            message: message.into(),
        }
    }

    /// 错误分类（#11 §3 / #14 §5）：5xx / 429 / 超时可重试，其余 4xx 终态。
    pub fn from_http(status: u16, body: &str) -> Self {
        let class = if status == 429 || status == 408 || status >= 500 {
            ErrorClass::Retryable
        } else {
            ErrorClass::Terminal
        };
        Self {
            class,
            http_status: Some(status),
            message: truncate(body, 500),
        }
    }

    pub fn failure_reason(&self) -> String {
        match self.http_status {
            Some(status) => format!("http_{status}"),
            None if self.message.contains("超时") || self.message.contains("timeout") => {
                "timeout".to_string()
            }
            None => "network".to_string(),
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect::<String>() + "…"
}

pub struct ChatClient {
    http: reqwest::Client,
    cfg: ProviderConfig,
}

impl ChatClient {
    pub fn new(cfg: ProviderConfig) -> Result<Self, ProviderError> {
        let mut builder = reqwest::Client::builder()
            .timeout(cfg.request_timeout)
            .user_agent(format!("aetherhud-eval/0.1 ({} provider)", cfg.provider));
        builder = match cfg
            .proxy
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            Some(proxy) => builder.proxy(
                reqwest::Proxy::all(proxy)
                    .map_err(|e| ProviderError::terminal(format!("代理配置非法：{e}")))?,
            ),
            None => builder.no_proxy(),
        };
        let http = builder
            .build()
            .map_err(|e| ProviderError::terminal(format!("HTTP 客户端初始化失败：{e}")))?;
        Ok(Self { http, cfg })
    }

    pub fn config(&self) -> &ProviderConfig {
        &self.cfg
    }

    pub fn build_body(&self, messages: &[Message]) -> Value {
        match self.cfg.message_type {
            MessageType::ChatCompletions => self.build_chat_completions_body(messages),
            MessageType::Responses => self.build_responses_body(messages),
        }
    }

    fn build_chat_completions_body(&self, messages: &[Message]) -> Value {
        let mut body = Map::new();
        body.insert("model".to_string(), Value::String(self.cfg.model.clone()));
        body.insert(
            "messages".to_string(),
            serde_json::to_value(messages).unwrap_or(Value::Array(vec![])),
        );
        body.insert("temperature".to_string(), Value::from(self.cfg.temperature));
        body.insert("max_tokens".to_string(), Value::from(self.cfg.max_tokens));
        body.insert(
            "response_format".to_string(),
            serde_json::json!({"type": "json_object"}),
        );
        for (k, v) in &self.cfg.extra_body {
            body.insert(k.clone(), v.clone());
        }
        Value::Object(body)
    }

    /// OpenAI Responses API 的**最小兼容实现**：每条 message 变成 input 里的一项
    /// （system 也可以作为首项，role=system），输出格式用 text.format=json_object。
    ///
    /// 不做 Responses 的高级特性（工具调用、reasoning、多轮 item 引用）；目标只是让
    /// 只支持 /responses 的 OpenAI 兼容端点也能跑同一套分类 prompt。
    fn build_responses_body(&self, messages: &[Message]) -> Value {
        let input: Vec<Value> = messages
            .iter()
            .map(|message| {
                serde_json::json!({
                    "role": message.role,
                    "content": message.content,
                })
            })
            .collect();
        let mut body = Map::new();
        body.insert("model".to_string(), Value::String(self.cfg.model.clone()));
        body.insert("input".to_string(), Value::Array(input));
        body.insert("temperature".to_string(), Value::from(self.cfg.temperature));
        body.insert(
            "max_output_tokens".to_string(),
            Value::from(self.cfg.max_tokens),
        );
        body.insert(
            "text".to_string(),
            serde_json::json!({"format": {"type": "json_object"}}),
        );
        for (k, v) in &self.cfg.extra_body {
            body.insert(k.clone(), v.clone());
        }
        Value::Object(body)
    }

    pub async fn chat(&self, messages: &[Message]) -> Result<ChatResponse, ProviderError> {
        let body = self.build_body(messages);
        let response = self
            .http
            .post(self.cfg.endpoint())
            .bearer_auth(&self.cfg.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError {
                        class: ErrorClass::Retryable,
                        http_status: None,
                        message: format!("请求超时：{e}"),
                    }
                } else {
                    ProviderError {
                        class: ErrorClass::Retryable,
                        http_status: None,
                        message: format!("网络错误：{e}"),
                    }
                }
            })?;
        let status = response.status().as_u16();
        let text = response.text().await.map_err(|e| ProviderError {
            class: ErrorClass::Retryable,
            http_status: Some(status),
            message: format!("读取响应失败：{e}"),
        })?;
        if !(200..300).contains(&status) {
            return Err(ProviderError::from_http(status, &text));
        }
        match self.cfg.message_type {
            MessageType::ChatCompletions => parse_chat_response(&text),
            MessageType::Responses => parse_responses_response(&text),
        }
    }
}

/// Responses API 响应解析：优先顶层 `output_text`，否则拼接
/// `output[].content[]` 里 `type == "output_text"` 的 text。
fn parse_responses_response(text: &str) -> Result<ChatResponse, ProviderError> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        ProviderError::terminal(format!(
            "provider 响应不是 JSON：{e}（{}）",
            truncate(text, 300)
        ))
    })?;
    let content = value
        .get("output_text")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
        .or_else(|| collect_output_text(&value))
        .ok_or_else(|| {
            ProviderError::terminal(format!(
                "provider 响应缺少 output_text 或 output[].content[].output_text（{}）",
                truncate(text, 300)
            ))
        })?;
    let usage = value
        .get("usage")
        .map(|usage| Usage {
            // Responses 用 input_tokens/output_tokens；兼容网关也会回 prompt/completion。
            prompt_tokens: usage
                .get("input_tokens")
                .or_else(|| usage.get("prompt_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
            completion_tokens: usage
                .get("output_tokens")
                .or_else(|| usage.get("completion_tokens"))
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
        .unwrap_or_default();
    // 尽力而为：正常完成是 status=completed；被截断时取 incomplete_details.reason。
    let finish_reason = value
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .pointer("/incomplete_details/reason")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(ChatResponse {
        content,
        usage,
        finish_reason,
        model,
    })
}

fn collect_output_text(value: &Value) -> Option<String> {
    let mut pieces: Vec<&str> = Vec::new();
    if let Some(items) = value.get("output").and_then(Value::as_array) {
        for item in items {
            if let Some(parts) = item.get("content").and_then(Value::as_array) {
                for part in parts {
                    if part.get("type").and_then(Value::as_str) == Some("output_text") {
                        if let Some(text) = part.get("text").and_then(Value::as_str) {
                            pieces.push(text);
                        }
                    }
                }
            }
        }
    }
    if pieces.is_empty() {
        None
    } else {
        Some(pieces.concat())
    }
}

fn parse_chat_response(text: &str) -> Result<ChatResponse, ProviderError> {
    let value: Value = serde_json::from_str(text).map_err(|e| {
        ProviderError::terminal(format!(
            "provider 响应不是 JSON：{e}（{}）",
            truncate(text, 300)
        ))
    })?;
    let content = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| {
            ProviderError::terminal(format!(
                "provider 响应缺少 choices[0].message.content（{}）",
                truncate(text, 300)
            ))
        })?;
    let usage = value
        .get("usage")
        .map(|u| Usage {
            prompt_tokens: u.get("prompt_tokens").and_then(Value::as_u64).unwrap_or(0),
            completion_tokens: u
                .get("completion_tokens")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        })
        .unwrap_or_default();
    let finish_reason = value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|c| c.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok(ChatResponse {
        content,
        usage,
        finish_reason,
        model,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(message_type: MessageType) -> ProviderConfig {
        ProviderConfig {
            provider: "dashscope".into(),
            base_url: "https://example.com/v1/".into(),
            model: "m".into(),
            api_key: "k".into(),
            message_type,
            temperature: 0.0,
            max_tokens: 400,
            request_timeout: Duration::from_secs(30),
            proxy: None,
            extra_body: Map::new(),
        }
    }

    #[test]
    fn chat_completions_endpoint_and_body_are_unchanged() {
        let client = ChatClient::new(config(MessageType::ChatCompletions)).unwrap();
        assert_eq!(
            client.config().endpoint(),
            "https://example.com/v1/chat/completions"
        );
        let body = client.build_body(&[Message::system("s"), Message::user("hi")]);
        assert_eq!(body["temperature"], 0.0);
        assert_eq!(body["max_tokens"], 400);
        assert_eq!(body["response_format"]["type"], "json_object");
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][1]["role"], "user");
        assert!(body.get("input").is_none());
    }

    #[test]
    fn a_full_chat_completions_url_is_used_as_is() {
        let mut cfg = config(MessageType::ChatCompletions);
        cfg.base_url = "https://example.com/v1/chat/completions".into();
        let client = ChatClient::new(cfg).unwrap();
        assert_eq!(
            client.config().endpoint(),
            "https://example.com/v1/chat/completions"
        );
    }

    #[test]
    fn responses_endpoint_and_body_use_the_responses_shape() {
        let client = ChatClient::new(config(MessageType::Responses)).unwrap();
        assert_eq!(
            client.config().endpoint(),
            "https://example.com/v1/responses"
        );

        let body = client.build_body(&[Message::system("s"), Message::user("hi")]);
        assert_eq!(body["model"], "m");
        assert_eq!(body["input"][0]["role"], "system");
        assert_eq!(body["input"][0]["content"], "s");
        assert_eq!(body["input"][1]["role"], "user");
        assert_eq!(body["input"][1]["content"], "hi");
        assert_eq!(body["temperature"], 0.0);
        assert_eq!(body["max_output_tokens"], 400);
        assert_eq!(body["text"]["format"]["type"], "json_object");
        assert!(body.get("messages").is_none());
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn a_full_responses_url_is_used_as_is() {
        let mut cfg = config(MessageType::Responses);
        cfg.base_url = "https://example.com/v1/responses/".into();
        let client = ChatClient::new(cfg).unwrap();
        assert_eq!(
            client.config().endpoint(),
            "https://example.com/v1/responses"
        );
    }

    #[test]
    fn parses_the_top_level_output_text_field() {
        let resp = parse_responses_response(
            r#"{"model":"gpt-4o-mini","status":"completed","output_text":"{\"l1\":\"a\"}","usage":{"input_tokens":10,"output_tokens":5}}"#,
        )
        .unwrap();
        assert_eq!(resp.content, r#"{"l1":"a"}"#);
        assert_eq!(resp.usage.prompt_tokens, 10);
        assert_eq!(resp.usage.completion_tokens, 5);
        assert_eq!(resp.usage.total(), 15);
        assert_eq!(resp.finish_reason.as_deref(), Some("completed"));
        assert_eq!(resp.model.as_deref(), Some("gpt-4o-mini"));
    }

    #[test]
    fn parses_output_items_when_output_text_is_absent() {
        let resp = parse_responses_response(
            r#"{"status":"incomplete","incomplete_details":{"reason":"max_output_tokens"},
                "output":[{"type":"reasoning","content":[]},
                          {"type":"message","content":[
                             {"type":"output_text","text":"{\"l1\":"},
                             {"type":"output_text","text":"\"a\"}"}]}]}"#,
        )
        .unwrap();
        assert_eq!(resp.content, r#"{"l1":"a"}"#);
        assert_eq!(resp.usage.total(), 0, "usage 缺失按 0 计");
        assert_eq!(resp.finish_reason.as_deref(), Some("incomplete"));
    }

    #[test]
    fn responses_without_output_text_are_terminal_and_readable() {
        let err = parse_responses_response(r#"{"output":[{"content":[]}]}"#).unwrap_err();
        assert_eq!(err.class, ErrorClass::Terminal);
        assert!(err.message.contains("output_text"), "{}", err.message);
    }

    #[test]
    fn http_error_classification_matches_11() {
        assert_eq!(
            ProviderError::from_http(500, "x").class,
            ErrorClass::Retryable
        );
        assert_eq!(
            ProviderError::from_http(429, "x").class,
            ErrorClass::Retryable
        );
        assert_eq!(
            ProviderError::from_http(401, "x").class,
            ErrorClass::Terminal
        );
        assert_eq!(
            ProviderError::from_http(400, "x").class,
            ErrorClass::Terminal
        );
        assert_eq!(
            ProviderError::from_http(500, "x").failure_reason(),
            "http_500"
        );
    }

    #[test]
    fn parses_usage_and_content() {
        let resp = parse_chat_response(
            r#"{"model":"qwen-flash","choices":[{"message":{"content":"{}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        )
        .unwrap();
        assert_eq!(resp.content, "{}");
        assert_eq!(resp.usage.total(), 15);
        assert_eq!(resp.finish_reason.as_deref(), Some("stop"));
    }

    #[test]
    fn malformed_response_is_terminal() {
        let err = parse_chat_response(r#"{"choices":[]}"#).unwrap_err();
        assert_eq!(err.class, ErrorClass::Terminal);
    }
}
