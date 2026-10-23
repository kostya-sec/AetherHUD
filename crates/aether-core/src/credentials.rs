//! 凭证与 Keychain 边界（#15 规格 / #29）。
//!
//! 三条硬规则：
//! 1. 密钥本体只进 OS Keychain（service 固定 `aetherhud`），数据库与设置只存
//!    `keychain://aetherhud/<account>` 引用；
//! 2. 没有 Key 时调用方拿到 `None`（或可读的"未配置"），功能降级而不是崩溃；
//! 3. 对外错误串与日志只出现引用/账号名，永远不出现密钥明文——验证结果返回前
//!    还会再按已知密钥做一次擦除（`redact`）。
//!
//! 本模块不读系统时钟、不碰 Tauri：Keychain / Clock / HttpTransport 都由壳注入。

use serde::{Deserialize, Serialize};

use crate::adapters::{secret_ref, Clock, HttpRequest, HttpTransport, Keychain};
use crate::provider::ProviderProfile;
use crate::storage::{Database, StorageError};

/// Jina 验证用的一次读（#15 §4）。
pub const JINA_READ_PROBE_URL: &str = "https://r.jina.ai/https://example.com";
/// 飞书 tenant_access_token 交换端点（#15 §4 / #5）。
pub const FEISHU_TENANT_TOKEN_URL: &str =
    "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
/// 凭证验证超时：Jina 一次读平均 7.9s，给足余量但不让设置页无限转圈。
pub const VALIDATION_TIMEOUT_MS: u64 = 20_000;

/// 只保留前 4 后 4 的脱敏串（#15 §2.1）；8 字符及以下整体遮蔽。
pub fn redact_secret(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.is_empty() {
        return String::new();
    }
    if chars.len() <= 8 {
        return "****".to_string();
    }
    let head: String = chars[..4].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

/// 把文本里出现的已知密钥替换为脱敏串；用于"错误串可能回显请求头"的兜底。
pub fn scrub_secrets(text: &str, secrets: &[String]) -> String {
    let mut scrubbed = text.to_string();
    for secret in secrets {
        let secret = secret.trim();
        if secret.is_empty() || !scrubbed.contains(secret) {
            continue;
        }
        scrubbed = scrubbed.replace(secret, &redact_secret(secret));
    }
    scrubbed
}

/// 凭证验证目标（凭证区里可点的三个"验证"）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ValidationTarget {
    Classifier,
    Jina,
    Feishu,
}

/// 验证结果：只返回可读结论，不含任何密钥（错误串已脱敏）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidationOutcome {
    pub ok: bool,
    pub detail: String,
}

/// 凭证种类：account 名按 docs/spec/api-keys.md §2.1。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialKind {
    ClassifierApiKey,
    JinaApiKey,
    FeishuAppId,
    FeishuAppSecret,
}

impl CredentialKind {
    /// 凭证区逐项顺序（保存/清除/验证都以它为准）。
    pub const ALL: [CredentialKind; 4] = [
        Self::ClassifierApiKey,
        Self::JinaApiKey,
        Self::FeishuAppId,
        Self::FeishuAppSecret,
    ];

    /// 系统钥匙串里的 account 名（#15 规格固定值）。
    pub fn account(self) -> &'static str {
        match self {
            Self::ClassifierApiKey => "classifier.api_key",
            Self::JinaApiKey => "jina.api_key",
            Self::FeishuAppId => "feishu.app_id",
            Self::FeishuAppSecret => "feishu.app_secret",
        }
    }

    /// 设置页展示名（界面语言 v1 仅简体中文）。
    pub fn label(self) -> &'static str {
        match self {
            Self::ClassifierApiKey => "分类器 API Key",
            Self::JinaApiKey => "Jina API Key",
            Self::FeishuAppId => "飞书 App ID",
            Self::FeishuAppSecret => "飞书 App Secret",
        }
    }

    /// settings 表的引用键（#10 §3.8 的 `classifier.api_key_ref` 命名）。
    pub fn reference_key(self) -> String {
        format!("{}_ref", self.account())
    }

    /// 可入库/可展示的引用串；永不含密钥。
    pub fn reference(self) -> String {
        secret_ref(self.account())
    }
}

/// 凭据状态：界面只显示这三态，绝不显示密钥。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialState {
    Configured,
    NotConfigured,
    Unavailable,
}

/// 一项凭证的可序列化状态（IPC 直接返回给设置页）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub kind: CredentialKind,
    pub account: &'static str,
    pub label: &'static str,
    pub state: CredentialState,
    pub reference: String,
}

/// 凭证操作错误：只带账号/引用，不带密钥。
#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("凭证不能为空")]
    EmptySecret,
    #[error("无法访问系统钥匙串（{account}）：{reason}")]
    Keychain {
        account: &'static str,
        reason: String,
    },
    #[error("写入凭证引用失败：{0}")]
    Storage(#[from] StorageError),
}

/// 凭证服务：Keychain + settings 表的组合边界，壳与测试都通过它读写凭证。
pub struct CredentialService<'a> {
    keychain: &'a dyn Keychain,
    database: &'a Database,
    clock: &'a dyn Clock,
}

impl<'a> CredentialService<'a> {
    pub fn new(keychain: &'a dyn Keychain, database: &'a Database, clock: &'a dyn Clock) -> Self {
        Self {
            keychain,
            database,
            clock,
        }
    }

    /// 凭证区四行状态；读取钥匙串失败降级为 Unavailable，绝不让设置页崩溃。
    pub fn statuses(&self) -> Vec<CredentialStatus> {
        CredentialKind::ALL.map(|kind| self.status(kind)).to_vec()
    }

    pub fn status(&self, kind: CredentialKind) -> CredentialStatus {
        let state = match self.keychain.get_secret(kind.account()) {
            Ok(Some(secret)) if !secret.trim().is_empty() => CredentialState::Configured,
            Ok(_) => CredentialState::NotConfigured,
            Err(_) => CredentialState::Unavailable,
        };
        CredentialStatus {
            kind,
            account: kind.account(),
            label: kind.label(),
            state,
            reference: self.stored_reference(kind),
        }
    }

    /// DB 里存的引用（若存在且指向同一 account）；否则用规范引用。
    fn stored_reference(&self, kind: CredentialKind) -> String {
        self.database
            .setting_get::<String>(&kind.reference_key())
            .ok()
            .flatten()
            .filter(|reference| {
                crate::adapters::parse_secret_ref(reference) == Some(kind.account())
            })
            .unwrap_or_else(|| kind.reference())
    }

    /// 保存一项凭证：密钥写系统钥匙串，设置表只落 `keychain://` 引用。
    ///
    /// 钥匙串写失败时绝不落引用；引用写失败时回滚钥匙串，避免"有密钥没引用"的半写状态。
    pub fn save(
        &self,
        kind: CredentialKind,
        secret: &str,
    ) -> Result<CredentialStatus, CredentialError> {
        let secret = secret.trim();
        if secret.is_empty() {
            return Err(CredentialError::EmptySecret);
        }
        self.keychain
            .set_secret(kind.account(), secret)
            .map_err(|error| CredentialError::Keychain {
                account: kind.account(),
                reason: error.to_string(),
            })?;
        if let Err(error) = self.database.setting_set(
            &kind.reference_key(),
            &kind.reference(),
            self.clock.now_ms(),
        ) {
            let _ = self.keychain.delete_secret(kind.account());
            return Err(error.into());
        }
        Ok(self.status(kind))
    }

    /// 清除一项凭证：先删设置引用（无外部副作用，失败即返回），再删钥匙串密钥；
    /// 钥匙串删除失败时把引用写回，避免"有密钥没引用"的半写状态。未配置时幂等。
    pub fn clear(&self, kind: CredentialKind) -> Result<CredentialStatus, CredentialError> {
        let reference_key = kind.reference_key();
        self.database.setting_delete(&reference_key)?;
        match self.keychain.delete_secret(kind.account()) {
            Ok(()) => Ok(self.status(kind)),
            Err(error) => {
                let _ = self.database.setting_set(
                    &reference_key,
                    &kind.reference(),
                    self.clock.now_ms(),
                );
                Err(CredentialError::Keychain {
                    account: kind.account(),
                    reason: error.to_string(),
                })
            }
        }
    }

    /// 读取密钥本体（只在验证/调用外部服务时使用）；未配置返回 None。
    pub fn secret(&self, kind: CredentialKind) -> Result<Option<String>, CredentialError> {
        match self.keychain.get_secret(kind.account()) {
            Ok(Some(secret)) if !secret.trim().is_empty() => Ok(Some(secret)),
            Ok(_) => Ok(None),
            Err(error) => Err(CredentialError::Keychain {
                account: kind.account(),
                reason: error.to_string(),
            }),
        }
    }

    /// 验证凭证连通性：百炼 GET /models、Jina 一次读、飞书 tenant_access_token。
    ///
    /// 失败一律返回可读原因（代理 / Key / 权限），且返回前按本次用到的密钥做一次
    /// 擦除——即使传输层的错误串或上游 msg 回显了密钥也不会泄漏。
    pub async fn validate(
        &self,
        target: ValidationTarget,
        transport: &dyn HttpTransport,
    ) -> ValidationOutcome {
        match target {
            ValidationTarget::Classifier => self.validate_classifier(transport).await,
            ValidationTarget::Jina => self.validate_jina(transport).await,
            ValidationTarget::Feishu => self.validate_feishu(transport).await,
        }
    }

    /// 验证分类器：用**活动供应商配置**（settings 里的 base URL）做 GET /models。
    ///
    /// 配置未保存时回落到百炼默认值，行为与 #29 等价；自定义供应商没填 base URL
    /// 时直接给可读提示，不发请求。
    async fn validate_classifier(&self, transport: &dyn HttpTransport) -> ValidationOutcome {
        let profile = ProviderProfile::load(self.database);
        let provider = profile.preset.label();
        if profile.base_url.trim().is_empty() {
            return failure(
                "请先在凭证区保存供应商配置（Base URL 与模型），再验证分类器 Key",
                &[],
            );
        }
        let key = match self.secret(CredentialKind::ClassifierApiKey) {
            Ok(Some(key)) => key,
            Ok(None) => {
                let detail = format!(
                    "尚未配置「{provider}」API Key（分类器不可用，条目会落到 00 未分类 / 待人工归档）"
                );
                return failure(detail, &[]);
            }
            Err(error) => return failure(format!("无法读取系统钥匙串：{error}"), &[]),
        };
        let secrets = [key.clone()];
        let base = profile.base_url.trim_end_matches('/').to_string();
        let url = models_endpoint(&base);
        let request = HttpRequest::get(url)
            .with_header("authorization", format!("Bearer {key}"))
            .with_timeout_ms(VALIDATION_TIMEOUT_MS);
        let response = match transport.request(request).await {
            Ok(response) => response,
            Err(error) => {
                return failure(
                    format!(
                        "无法连接「{provider}」：{error}。若本机需要代理，请在「网络」设置中配置代理后重试。"
                    ),
                    &secrets,
                )
            }
        };
        match response.status {
            200 => {
                let models = serde_json::from_str::<serde_json::Value>(&response.body)
                    .ok()
                    .and_then(|body| {
                        body.get("data")
                            .and_then(|data| data.as_array())
                            .map(Vec::len)
                    });
                match models {
                    Some(count) => success(format!(
                        "「{provider}」连接正常（{count} 个模型，模型 {}）",
                        profile.model
                    )),
                    None => success(format!("「{provider}」连接正常（模型 {}）", profile.model)),
                }
            }
            401 | 403 => failure(
                format!(
                    "「{provider}」Key 无效或没有访问权限（HTTP {}）。请重新复制 API Key。",
                    response.status
                ),
                &secrets,
            ),
            404 => failure(
                format!(
                    "「{provider}」兼容端点不存在（HTTP 404）。请检查 Base URL（{base}）与模型（{}）。",
                    profile.model
                ),
                &secrets,
            ),
            429 => failure(
                format!("「{provider}」请求过于频繁（HTTP 429），请稍后重试。"),
                &secrets,
            ),
            status if status >= 500 => failure(
                format!("「{provider}」服务端暂时不可用（HTTP {status}），请稍后重试。"),
                &secrets,
            ),
            status => failure(
                format!("「{provider}」验证失败（HTTP {status}）。"),
                &secrets,
            ),
        }
    }

    async fn validate_jina(&self, transport: &dyn HttpTransport) -> ValidationOutcome {
        let key = match self.secret(CredentialKind::JinaApiKey) {
            Ok(Some(key)) => key,
            Ok(None) => {
                return failure(
                    "尚未配置 Jina API Key（未配置时抓取走匿名额度；可先保存再验证）",
                    &[],
                )
            }
            Err(error) => return failure(format!("无法读取系统钥匙串：{error}"), &[]),
        };
        let secrets = [key.clone()];
        let request = HttpRequest::get(JINA_READ_PROBE_URL)
            .with_header("authorization", format!("Bearer {key}"))
            .with_timeout_ms(VALIDATION_TIMEOUT_MS);
        let response = match transport.request(request).await {
            Ok(response) => response,
            Err(error) => {
                return failure(
                    format!(
                        "无法连接 Jina：{error}。本机访问 r.jina.ai 通常需要「网络」设置中的代理。"
                    ),
                    &secrets,
                )
            }
        };
        match response.status {
            200 if !response.body.trim().is_empty() => success("Jina Reader 可用（一次读取成功）"),
            200 => failure(
                "Jina Reader 返回了空内容（目标站点可能拒绝渲染），请稍后重试。",
                &secrets,
            ),
            401..=403 => failure(
                format!(
                    "Jina Key 无效或额度不足（HTTP {}）。请检查 Key 与剩余额度。",
                    response.status
                ),
                &secrets,
            ),
            429 => failure("Jina 请求过于频繁（HTTP 429），请稍后重试。", &secrets),
            404 => failure("Jina Reader 端点不存在（HTTP 404），请稍后重试。", &secrets),
            status if status >= 500 => failure(
                format!("Jina 服务端暂时不可用（HTTP {status}），请稍后重试。"),
                &secrets,
            ),
            status => failure(format!("Jina 验证失败（HTTP {status}）。"), &secrets),
        }
    }

    async fn validate_feishu(&self, transport: &dyn HttpTransport) -> ValidationOutcome {
        let app_id = match self.secret(CredentialKind::FeishuAppId) {
            Ok(Some(secret)) => secret,
            Ok(None) => return failure("请先保存飞书 App ID 与 App Secret", &[]),
            Err(error) => return failure(format!("无法读取系统钥匙串：{error}"), &[]),
        };
        let app_secret = match self.secret(CredentialKind::FeishuAppSecret) {
            Ok(Some(secret)) => secret,
            Ok(None) => return failure("请先保存飞书 App ID 与 App Secret", &[]),
            Err(error) => return failure(format!("无法读取系统钥匙串：{error}"), &[]),
        };
        let secrets = [app_id.clone(), app_secret.clone()];
        let body = serde_json::json!({
            "app_id": app_id,
            "app_secret": app_secret,
        })
        .to_string();
        let request = HttpRequest::post(FEISHU_TENANT_TOKEN_URL, body)
            .with_header("content-type", "application/json; charset=utf-8")
            .with_timeout_ms(VALIDATION_TIMEOUT_MS);
        let response = match transport.request(request).await {
            Ok(response) => response,
            Err(error) => {
                return failure(
                    format!("无法连接飞书：{error}。若本机需要代理，请检查「网络」设置。"),
                    &secrets,
                )
            }
        };
        match response.status {
            200 => match serde_json::from_str::<FeishuTokenResponse>(&response.body) {
                Ok(parsed) if parsed.code == 0 && !parsed.tenant_access_token.is_empty() => success(
                    format!(
                        "飞书 tenant_access_token 获取成功（有效期 {} 秒）",
                        parsed.expire
                    ),
                ),
                Ok(parsed) if parsed.code != 0 => failure(
                    format!(
                        "飞书返回错误（code {}）：{}。请检查 App ID / App Secret 是否有效、应用是否已发布。",
                        parsed.code, parsed.msg
                    ),
                    &secrets,
                ),
                Ok(_) => failure("飞书返回了空 token，请检查应用发布状态。", &secrets),
                Err(_) => failure("飞书响应不是合法 JSON，请稍后重试。", &secrets),
            },
            401 | 403 => failure(
                format!(
                    "飞书 App ID / App Secret 无效，或应用没有相应权限（HTTP {}）。",
                    response.status
                ),
                &secrets,
            ),
            400 => failure(
                "飞书拒绝了请求（HTTP 400）。请检查 App ID / App Secret 是否完整。",
                &secrets,
            ),
            429 => failure("飞书请求过于频繁（HTTP 429），请稍后重试。", &secrets),
            status if status >= 500 => failure(
                format!("飞书服务端暂时不可用（HTTP {status}），请稍后重试。"),
                &secrets,
            ),
            status => failure(format!("飞书验证失败（HTTP {status}）。"), &secrets),
        }
    }

    /// 记录本次凭证变更时间（供设置页"最近保存"与测试断言；本票只落库不改 UI 文案）。
    pub fn touched_at(&self) -> i64 {
        self.clock.now_ms()
    }
}

/// 分类器验证用的 `/models` 端点：Base URL 允许直接填完整对话端点
/// （`…/chat/completions` 或 `…/responses`），验证时先退回服务根再拼 `/models`。
fn models_endpoint(base_url: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    let root = base
        .strip_suffix("/chat/completions")
        .or_else(|| base.strip_suffix("/responses"))
        .unwrap_or(base);
    format!("{root}/models")
}

/// 飞书 tenant_access_token 响应（只取验证需要的字段；未知字段忽略）。
#[derive(Debug, Deserialize)]
struct FeishuTokenResponse {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: String,
    #[serde(default)]
    tenant_access_token: String,
    #[serde(default)]
    expire: i64,
}

fn success(detail: impl Into<String>) -> ValidationOutcome {
    ValidationOutcome {
        ok: true,
        detail: detail.into(),
    }
}

fn failure(detail: impl Into<String>, secrets: &[String]) -> ValidationOutcome {
    ValidationOutcome {
        ok: false,
        detail: scrub_secrets(&detail.into(), secrets),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::testing::{
        FakeClock, FakeHttpTransport, MemoryKeychain, UnavailableKeychain,
    };
    use crate::adapters::{HttpMethod, HttpResponse};
    use crate::provider::{MessageType, ProviderPreset, PROVIDER_SETTING};

    fn service_fixture<'a>(
        keychain: &'a MemoryKeychain,
        database: &'a Database,
        clock: &'a FakeClock,
    ) -> CredentialService<'a> {
        CredentialService::new(keychain, database, clock)
    }

    #[test]
    fn fresh_database_reports_every_credential_as_not_configured() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = service_fixture(&keychain, &database, &clock);

        let statuses = service.statuses();

        assert_eq!(statuses.len(), 4);
        assert_eq!(
            statuses
                .iter()
                .map(|status| status.account)
                .collect::<Vec<_>>(),
            vec![
                "classifier.api_key",
                "jina.api_key",
                "feishu.app_id",
                "feishu.app_secret"
            ]
        );
        assert!(statuses
            .iter()
            .all(|status| status.state == CredentialState::NotConfigured));
        assert_eq!(
            statuses[0].reference,
            "keychain://aetherhud/classifier.api_key"
        );
        assert_eq!(statuses[0].label, "分类器 API Key");
    }

    #[test]
    fn a_secret_already_in_the_keychain_shows_as_configured() {
        let keychain = MemoryKeychain::new();
        keychain.set_secret("jina.api_key", "jina-key").unwrap();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = service_fixture(&keychain, &database, &clock);

        let status = service.status(CredentialKind::JinaApiKey);

        assert_eq!(status.state, CredentialState::Configured);
        assert!(service.statuses().iter().all(|status| {
            status.state
                == if status.account == "jina.api_key" {
                    CredentialState::Configured
                } else {
                    CredentialState::NotConfigured
                }
        }));
    }

    #[test]
    fn saving_a_credential_stores_the_secret_in_the_keychain_and_only_a_reference_in_the_database()
    {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = service_fixture(&keychain, &database, &clock);

        let status = service
            .save(
                CredentialKind::ClassifierApiKey,
                "  sk-test-secret-0123456789  ",
            )
            .unwrap();

        assert_eq!(status.state, CredentialState::Configured);
        assert_eq!(
            keychain
                .get_secret("classifier.api_key")
                .unwrap()
                .as_deref(),
            Some("sk-test-secret-0123456789"),
            "两端的空白应被去掉后存入钥匙串"
        );
        assert_eq!(
            database
                .setting_get::<String>("classifier.api_key_ref")
                .unwrap()
                .as_deref(),
            Some("keychain://aetherhud/classifier.api_key")
        );
        let dump = database.settings_snapshot();
        assert!(
            dump.iter()
                .all(|(_, value)| !value.contains("sk-test-secret")),
            "设置表只能有引用，不能有密钥明文：{dump:?}"
        );
    }

    #[test]
    fn empty_secrets_are_rejected_before_they_touch_the_keychain() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = service_fixture(&keychain, &database, &clock);

        let error = service.save(CredentialKind::JinaApiKey, "   ").unwrap_err();

        assert!(matches!(error, CredentialError::EmptySecret));
        assert!(!keychain.contains("jina.api_key"));
        assert!(database.settings_snapshot().is_empty());
    }

    #[test]
    fn clearing_a_credential_removes_the_secret_and_the_reference() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = service_fixture(&keychain, &database, &clock);
        service
            .save(CredentialKind::FeishuAppSecret, "feishu-secret")
            .unwrap();

        let status = service.clear(CredentialKind::FeishuAppSecret).unwrap();

        assert_eq!(status.state, CredentialState::NotConfigured);
        assert!(!keychain.contains("feishu.app_secret"));
        assert!(database
            .setting_get::<String>("feishu.app_secret_ref")
            .unwrap()
            .is_none());

        // 清除未配置的项是幂等的，不报错。
        let again = service.clear(CredentialKind::FeishuAppSecret).unwrap();
        assert_eq!(again.state, CredentialState::NotConfigured);
    }

    #[test]
    fn an_unavailable_keychain_degrades_status_and_reports_a_readable_error() {
        let keychain = UnavailableKeychain;
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = CredentialService::new(&keychain, &database, &clock);

        // 设置页不崩溃：四行都显示"不可用"。
        assert!(service
            .statuses()
            .iter()
            .all(|status| status.state == CredentialState::Unavailable));

        // 无 Key 时调用方得到 None 或可读错误，而不是 panic。
        assert!(service.secret(CredentialKind::JinaApiKey).is_err());

        let error = service
            .save(CredentialKind::ClassifierApiKey, "sk-should-not-leak")
            .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("钥匙串"), "错误要可读：{message}");
        assert!(!message.contains("sk-should-not-leak"));
        assert!(
            database.settings_snapshot().is_empty(),
            "钥匙串写入失败时不能留下引用"
        );
    }

    fn http_ok(body: &str) -> HttpResponse {
        HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.to_string(),
        }
    }

    fn http_status(status: u16) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: String::new(),
        }
    }

    fn configured_service_fixture<'a>(
        keychain: &'a MemoryKeychain,
        database: &'a Database,
        clock: &'a FakeClock,
    ) -> CredentialService<'a> {
        let service = service_fixture(keychain, database, clock);
        service
            .save(CredentialKind::ClassifierApiKey, "sk-bailian-secret")
            .unwrap();
        service
            .save(CredentialKind::JinaApiKey, "jina-secret-key")
            .unwrap();
        service
            .save(CredentialKind::FeishuAppId, "cli_test_app_id")
            .unwrap();
        service
            .save(CredentialKind::FeishuAppSecret, "feishu-app-secret")
            .unwrap();
        service
    }

    #[test]
    fn redacts_all_but_the_first_and_last_four_characters() {
        assert_eq!(redact_secret("sk-1234567890"), "sk-1…7890");
        assert_eq!(redact_secret("abc"), "****");
        assert_eq!(redact_secret(""), "");
        assert_eq!(redact_secret("12345678"), "****", "8 字符及以下整体遮蔽");
    }

    #[tokio::test]
    async fn classifier_validation_hits_models_with_the_bearer_key() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();
        transport.enqueue(
            format!("{}/models", crate::provider::BAILIAN_BASE_URL),
            http_ok(r#"{"data":[{"id":"qwen-flash"},{"id":"qwen-max"}]}"#),
        );

        let outcome = service
            .validate(ValidationTarget::Classifier, &transport)
            .await;

        assert!(outcome.ok, "{}", outcome.detail);
        assert!(outcome.detail.contains("2 个模型"), "{}", outcome.detail);
        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, HttpMethod::Get);
        assert_eq!(
            requests[0].url,
            format!("{}/models", crate::provider::BAILIAN_BASE_URL)
        );
        assert!(requests[0].headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("authorization") && value == "Bearer sk-bailian-secret"
        }));
    }

    #[tokio::test]
    async fn classifier_validation_honors_a_custom_base_url_from_settings() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        database
            .setting_set(
                crate::provider::BASE_URL_SETTING,
                &"https://dashscope-intl.aliyuncs.com/compatible-mode/v1/".to_string(),
                0,
            )
            .unwrap();
        let transport = FakeHttpTransport::new();
        transport.enqueue(
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1/models",
            http_ok(r#"{"data":[]}"#),
        );

        let outcome = service
            .validate(ValidationTarget::Classifier, &transport)
            .await;

        assert!(outcome.ok, "{}", outcome.detail);
    }

    #[tokio::test]
    async fn classifier_validation_uses_the_saved_custom_provider_profile() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        let profile = ProviderProfile {
            preset: ProviderPreset::Custom,
            base_url: "https://llm.example.com/openai/v1".to_string(),
            model: "my-model".to_string(),
            message_type: MessageType::Responses,
        };
        ProviderProfile::save(&database, &profile, 0).unwrap();
        let transport = FakeHttpTransport::new();
        transport.enqueue("https://llm.example.com/openai/v1/models", http_status(404));

        let outcome = service
            .validate(ValidationTarget::Classifier, &transport)
            .await;

        assert!(!outcome.ok);
        assert!(
            outcome.detail.contains("自定义（OpenAI 兼容）"),
            "失败信息要带上供应商：{}",
            outcome.detail
        );
        assert!(
            outcome.detail.contains("https://llm.example.com/openai/v1"),
            "失败信息要带上活动 Base URL：{}",
            outcome.detail
        );
        assert!(
            outcome.detail.contains("my-model"),
            "失败信息要带上活动模型：{}",
            outcome.detail
        );
        let requests = transport.requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(
            requests[0].url, "https://llm.example.com/openai/v1/models",
            "验证必须打活动供应商的 Base URL"
        );
        assert!(!outcome.detail.contains("sk-bailian-secret"));
    }

    #[tokio::test]
    async fn classifier_validation_accepts_a_full_chat_endpoint_as_base_url() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        ProviderProfile::save(
            &database,
            &ProviderProfile {
                preset: ProviderPreset::OpenAi,
                base_url: "https://api.openai.com/v1/chat/completions".to_string(),
                model: "gpt-4o-mini".to_string(),
                message_type: MessageType::ChatCompletions,
            },
            0,
        )
        .unwrap();
        let transport = FakeHttpTransport::new();
        transport.enqueue(
            "https://api.openai.com/v1/models",
            http_ok(r#"{"data":[]}"#),
        );

        let outcome = service
            .validate(ValidationTarget::Classifier, &transport)
            .await;

        assert!(outcome.ok, "{}", outcome.detail);
        assert_eq!(
            transport.requests()[0].url,
            "https://api.openai.com/v1/models",
            "完整对话端点应退回服务根再拼 /models"
        );
    }

    #[tokio::test]
    async fn custom_provider_without_a_saved_base_url_asks_to_configure_it_first() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        database
            .setting_set(PROVIDER_SETTING, &"custom".to_string(), 0)
            .unwrap();
        let transport = FakeHttpTransport::new();

        let outcome = service
            .validate(ValidationTarget::Classifier, &transport)
            .await;

        assert!(!outcome.ok);
        assert!(
            outcome.detail.contains("请先在凭证区保存供应商配置"),
            "{}",
            outcome.detail
        );
        assert!(
            transport.requests().is_empty(),
            "未配置 base URL 时不应发请求"
        );
    }

    #[tokio::test]
    async fn invalid_classifier_key_fails_with_a_readable_reason_and_no_secret() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();
        transport.enqueue(
            format!("{}/models", crate::provider::BAILIAN_BASE_URL),
            http_status(401),
        );

        let outcome = service
            .validate(ValidationTarget::Classifier, &transport)
            .await;

        assert!(!outcome.ok);
        assert!(outcome.detail.contains("Key 无效"), "{}", outcome.detail);
        assert!(!outcome.detail.contains("sk-bailian-secret"));
    }

    #[tokio::test]
    async fn a_network_error_points_at_the_proxy_setting_and_scrubs_echoed_credentials() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();
        transport.enqueue_error(
            JINA_READ_PROBE_URL,
            "connect failed while sending Authorization: Bearer jina-secret-key",
        );

        let outcome = service.validate(ValidationTarget::Jina, &transport).await;

        assert!(!outcome.ok);
        assert!(outcome.detail.contains("代理"), "{}", outcome.detail);
        assert!(
            !outcome.detail.contains("jina-secret-key"),
            "错误串必须脱敏：{}",
            outcome.detail
        );
        assert!(outcome.detail.contains("jina…-key") || outcome.detail.contains("…"));
    }

    #[tokio::test]
    async fn jina_validation_reads_one_page_with_the_stored_key() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();
        transport.enqueue(JINA_READ_PROBE_URL, http_ok("# Example Domain"));

        let outcome = service.validate(ValidationTarget::Jina, &transport).await;

        assert!(outcome.ok, "{}", outcome.detail);
        let requests = transport.requests();
        assert_eq!(requests[0].method, HttpMethod::Get);
        assert_eq!(requests[0].url, JINA_READ_PROBE_URL);
        assert!(requests[0].headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("authorization") && value == "Bearer jina-secret-key"
        }));
    }

    #[tokio::test]
    async fn jina_without_a_key_degrades_to_a_readable_hint_instead_of_calling_out() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();

        let outcome = service.validate(ValidationTarget::Jina, &transport).await;

        assert!(!outcome.ok);
        assert!(outcome.detail.contains("尚未配置"), "{}", outcome.detail);
        assert!(transport.requests().is_empty());
    }

    #[tokio::test]
    async fn feishu_validation_exchanges_app_credentials_for_a_tenant_token() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();
        transport.enqueue_post(
            FEISHU_TENANT_TOKEN_URL,
            http_ok(r#"{"code":0,"msg":"ok","tenant_access_token":"t-abc","expire":7200}"#),
        );

        let outcome = service.validate(ValidationTarget::Feishu, &transport).await;

        assert!(outcome.ok, "{}", outcome.detail);
        assert!(
            outcome.detail.contains("tenant_access_token"),
            "{}",
            outcome.detail
        );
        assert!(outcome.detail.contains("7200"));
        let requests = transport.requests();
        assert_eq!(requests[0].method, HttpMethod::Post);
        assert_eq!(requests[0].url, FEISHU_TENANT_TOKEN_URL);
        let body = requests[0].body.as_deref().unwrap_or_default();
        assert!(body.contains("cli_test_app_id"));
        assert!(body.contains("feishu-app-secret"), "凭据只在传输边界出现");
        assert!(requests[0].headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("content-type") && value.contains("application/json")
        }));
    }

    #[tokio::test]
    async fn feishu_error_code_becomes_a_readable_permission_hint() {
        let keychain = MemoryKeychain::new();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = configured_service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();
        transport.enqueue_post(
            FEISHU_TENANT_TOKEN_URL,
            http_ok(r#"{"code":10003,"msg":"invalid param"}"#),
        );

        let outcome = service.validate(ValidationTarget::Feishu, &transport).await;

        assert!(!outcome.ok);
        assert!(outcome.detail.contains("10003"), "{}", outcome.detail);
        assert!(outcome.detail.contains("invalid param"));
        assert!(outcome.detail.contains("App ID"), "{}", outcome.detail);
        assert!(!outcome.detail.contains("feishu-app-secret"));
    }

    #[tokio::test]
    async fn feishu_validation_needs_both_halves_before_calling_out() {
        let keychain = MemoryKeychain::new();
        keychain
            .set_secret("feishu.app_id", "cli_only_app_id")
            .unwrap();
        let database = Database::open_in_memory().unwrap();
        let clock = FakeClock::new(1_761_000_000_000);
        let service = service_fixture(&keychain, &database, &clock);
        let transport = FakeHttpTransport::new();

        let outcome = service.validate(ValidationTarget::Feishu, &transport).await;

        assert!(!outcome.ok);
        assert!(
            outcome.detail.contains("请先保存飞书 App ID 与 App Secret"),
            "{}",
            outcome.detail
        );
        assert!(transport.requests().is_empty());
    }
}
