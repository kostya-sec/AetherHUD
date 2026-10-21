//! OpenAI 兼容 Chat Completions provider（默认阿里百炼 DashScope，见 #15）。
//!
//! 单次调用只做一次 HTTP；重试由 [`crate::engine::ClassificationEngine`] 按 #11 策略驱动。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::prompt::Message;
use crate::retry::ErrorClass;

#[derive(Debug, Clone)]
pub struct ProviderConfig {
    /// provider 名称（报告参数；"dashscope" 时附加 enable_thinking=false）。
    pub provider: String,
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    pub temperature: f64,
    pub max_tokens: u32,
    pub request_timeout: Duration,
    pub proxy: Option<String>,
    pub extra_body: Map<String, Value>,
}

impl ProviderConfig {
    pub fn endpoint(&self) -> String {
        let base = self.base_url.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_string()
        } else {
            format!("{base}/chat/completions")
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
        parse_chat_response(&text)
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

    #[test]
    fn endpoint_normalizes() {
        let cfg = ProviderConfig {
            provider: "dashscope".into(),
            base_url: "https://example.com/v1/".into(),
            model: "m".into(),
            api_key: "k".into(),
            temperature: 0.0,
            max_tokens: 400,
            request_timeout: Duration::from_secs(30),
            proxy: None,
            extra_body: Map::new(),
        };
        let client = ChatClient::new(cfg).unwrap();
        assert_eq!(
            client.config().endpoint(),
            "https://example.com/v1/chat/completions"
        );
        let body = client.build_body(&[Message::user("hi")]);
        assert_eq!(body["temperature"], 0.0);
        assert_eq!(body["response_format"]["type"], "json_object");
        assert_eq!(body["messages"][0]["role"], "user");
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
