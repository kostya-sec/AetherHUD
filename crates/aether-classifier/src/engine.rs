//! 生产分类路径：组装 → provider 调用（#11 重试）→ 校验 → 1 次修复重试。
//!
//! 评测工具与生产 worker 共用本引擎；持久化由调用方负责（评测读数不写生产库）。

use std::time::Instant;

use serde::Serialize;

use crate::entry::EntryForClassify;
use crate::prompt::{repair_messages, Message, PromptConfig, PromptConfigError};
use crate::provider::{ChatClient, ProviderConfig, Usage};
use crate::retry::RetryPolicy;
use crate::schema::{validate_classification, Classification, ValidationIssue};
use crate::taxonomy::TaxonomyView;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CallPurpose {
    Initial,
    Repair,
}

#[derive(Debug, Clone, Serialize)]
pub struct CallRecord {
    pub purpose: CallPurpose,
    pub attempt: u32,
    pub ok: bool,
    pub http_status: Option<u16>,
    pub error: Option<String>,
    pub latency_ms: u64,
    pub usage: Usage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutcomeStatus {
    /// 合法输出（含弃权）。
    Ok,
    /// 1 次修复重试后仍格式非法（#14 §5：计入可用性，不计入准确率）。
    FormatInvalid,
    /// API 终态失败 / 重试穷尽。
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct ClassificationOutcome {
    pub status: OutcomeStatus,
    pub classification: Option<Classification>,
    pub abstained: bool,
    pub raw_output: Option<String>,
    pub first_raw_output: Option<String>,
    pub repair_used: bool,
    pub issues: Vec<ValidationIssue>,
    /// 首次输出触发修复重试的校验问题（修复成功时也保留，供 prompt 迭代分析）。
    pub repair_trigger_issues: Vec<ValidationIssue>,
    pub failure_reason: Option<String>,
    pub calls: Vec<CallRecord>,
    pub usage: Usage,
    pub elapsed_ms: u64,
}

impl ClassificationOutcome {
    pub fn total_calls(&self) -> usize {
        self.calls.len()
    }

    pub fn http_retry_count(&self) -> usize {
        let purposes = [CallPurpose::Initial, CallPurpose::Repair];
        let distinct = purposes
            .iter()
            .filter(|p| self.calls.iter().any(|c| c.purpose == **p))
            .count();
        self.calls.len().saturating_sub(distinct)
    }
}

pub struct ClassificationEngine {
    client: ChatClient,
    prompt: PromptConfig,
    tree: TaxonomyView,
    retry: RetryPolicy,
}

impl ClassificationEngine {
    pub fn try_new(
        client: ChatClient,
        prompt: PromptConfig,
        tree: TaxonomyView,
        retry: RetryPolicy,
    ) -> Result<Self, PromptConfigError> {
        prompt.validate(&tree)?;
        Ok(Self {
            client,
            prompt,
            tree,
            retry,
        })
    }

    pub fn prompt(&self) -> &PromptConfig {
        &self.prompt
    }

    pub fn tree(&self) -> &TaxonomyView {
        &self.tree
    }

    pub fn retry_policy(&self) -> &RetryPolicy {
        &self.retry
    }

    pub fn provider_config(&self) -> &ProviderConfig {
        self.client.config()
    }

    pub async fn classify(&self, entry: &EntryForClassify) -> ClassificationOutcome {
        let started = Instant::now();
        let messages = self.prompt.build_messages(&self.tree, entry);
        let mut calls: Vec<CallRecord> = Vec::new();
        let outcome = self.classify_inner(&messages, &mut calls).await;
        let mut usage = Usage::default();
        for call in &calls {
            usage.add(&call.usage);
        }
        ClassificationOutcome {
            status: outcome.status,
            classification: outcome.classification,
            abstained: outcome.abstained,
            raw_output: outcome.raw_output,
            first_raw_output: outcome.first_raw_output,
            repair_used: outcome.repair_used,
            issues: outcome.issues,
            repair_trigger_issues: outcome.repair_trigger_issues,
            failure_reason: outcome.failure_reason,
            calls,
            usage,
            elapsed_ms: started.elapsed().as_millis() as u64,
        }
    }

    async fn classify_inner(
        &self,
        messages: &[Message],
        calls: &mut Vec<CallRecord>,
    ) -> PartialOutcome {
        let first = match self
            .call_with_retry(messages, CallPurpose::Initial, calls)
            .await
        {
            Ok(response) => response,
            Err(error) => return PartialOutcome::unavailable(None, false, error.failure_reason()),
        };
        match validate_classification(&first.content, &self.tree) {
            Ok(validation) => PartialOutcome::ok(validation, None, Vec::new(), false),
            Err(first_error) => {
                let first_raw = first.content.clone();
                let first_issues = first_error.issues.clone();
                let repair_messages =
                    repair_messages(messages, &first.content, &first_error.issues);
                match self
                    .call_with_retry(&repair_messages, CallPurpose::Repair, calls)
                    .await
                {
                    Ok(response) => match validate_classification(&response.content, &self.tree) {
                        Ok(validation) => {
                            PartialOutcome::ok(validation, Some(first_raw), first_issues, true)
                        }
                        Err(second_error) => PartialOutcome {
                            status: OutcomeStatus::FormatInvalid,
                            classification: None,
                            abstained: false,
                            raw_output: Some(response.content),
                            first_raw_output: Some(first_raw),
                            repair_used: true,
                            issues: second_error.issues,
                            repair_trigger_issues: first_issues,
                            failure_reason: Some("format_invalid".to_string()),
                        },
                    },
                    Err(error) => PartialOutcome::unavailable(
                        Some(first.content),
                        true,
                        format!("repair_{}", error.failure_reason()),
                    ),
                }
            }
        }
    }

    async fn call_with_retry(
        &self,
        messages: &[Message],
        purpose: CallPurpose,
        calls: &mut Vec<CallRecord>,
    ) -> Result<crate::provider::ChatResponse, crate::provider::ProviderError> {
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            let started = Instant::now();
            match self.client.chat(messages).await {
                Ok(response) => {
                    calls.push(CallRecord {
                        purpose,
                        attempt,
                        ok: true,
                        http_status: None,
                        error: None,
                        latency_ms: started.elapsed().as_millis() as u64,
                        usage: response.usage.clone(),
                    });
                    return Ok(response);
                }
                Err(error) => {
                    calls.push(CallRecord {
                        purpose,
                        attempt,
                        ok: false,
                        http_status: error.http_status,
                        error: Some(error.to_string()),
                        latency_ms: started.elapsed().as_millis() as u64,
                        usage: Usage::default(),
                    });
                    if self.retry.should_retry(attempt, error.class) {
                        let delay = self.retry.delay_after_failure(attempt);
                        tokio::time::sleep(delay).await;
                        continue;
                    }
                    return Err(error);
                }
            }
        }
    }
}

struct PartialOutcome {
    status: OutcomeStatus,
    classification: Option<Classification>,
    abstained: bool,
    raw_output: Option<String>,
    first_raw_output: Option<String>,
    repair_used: bool,
    issues: Vec<ValidationIssue>,
    repair_trigger_issues: Vec<ValidationIssue>,
    failure_reason: Option<String>,
}

impl PartialOutcome {
    fn ok(
        validation: crate::schema::ValidationOutcome,
        first_raw_output: Option<String>,
        repair_trigger_issues: Vec<ValidationIssue>,
        repair_used: bool,
    ) -> Self {
        Self {
            status: OutcomeStatus::Ok,
            abstained: validation.abstained,
            classification: Some(validation.classification),
            raw_output: None,
            first_raw_output,
            repair_used,
            issues: Vec::new(),
            repair_trigger_issues,
            failure_reason: None,
        }
    }

    fn unavailable(raw: Option<String>, repair_used: bool, reason: String) -> Self {
        Self {
            status: OutcomeStatus::Unavailable,
            classification: None,
            abstained: false,
            raw_output: raw,
            first_raw_output: None,
            repair_used,
            issues: Vec::new(),
            repair_trigger_issues: Vec::new(),
            failure_reason: Some(reason),
        }
    }
}
