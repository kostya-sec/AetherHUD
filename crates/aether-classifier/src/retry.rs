//! #11 的重试语义：30s × 2^n ±20% jitter、封顶 30min、最多 8 次。
//!
//! 生产 pipeline worker 与 aether-eval 共用同一实现；评测串行调用时用
//! RetryPolicy 直接控制退避，测试可把 base_delay 置零。

use std::time::Duration;

/// 错误二分（#11 §3）：可重试 = 5xx / 429 / 超时 / 网络；终态 = 4xx / 401 / 格式非法。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Retryable,
    Terminal,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RetryPolicy {
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub max_attempts: u32,
    pub jitter_ratio: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            base_delay: Duration::from_secs(30),
            max_delay: Duration::from_secs(30 * 60),
            max_attempts: 8,
            jitter_ratio: 0.20,
        }
    }
}

impl RetryPolicy {
    /// 与 #11 一致的固定参数（测试与报告参数表使用）。
    pub fn from_defaults() -> Self {
        Self::default()
    }

    /// 一次失败之后的退避时长；failure_index 从 1 开始（第一次失败后等 30s）。
    pub fn delay_after_failure(&self, failure_index: u32) -> Duration {
        debug_assert!(failure_index >= 1);
        let exp = 2u32.saturating_pow(failure_index.saturating_sub(1));
        let raw = self.base_delay.saturating_mul(exp).min(self.max_delay);
        if self.jitter_ratio <= 0.0 {
            return raw;
        }
        // ±jitter：均匀分布在 [1 - jitter, 1 + jitter]
        let jitter = fastrand::f64() * 2.0 * self.jitter_ratio - self.jitter_ratio;
        raw.mul_f64((1.0 + jitter).max(0.0))
    }

    pub fn should_retry(&self, attempts_used: u32, class: ErrorClass) -> bool {
        class == ErrorClass::Retryable && attempts_used < self.max_attempts
    }

    /// 只用于报告：理论退避序列（不含 jitter）。
    pub fn nominal_schedule(&self) -> Vec<Duration> {
        (1..self.max_attempts)
            .map(|i| {
                let exp = 2u32.saturating_pow(i - 1);
                self.base_delay.saturating_mul(exp).min(self.max_delay)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nominal_schedule_matches_spec() {
        let p = RetryPolicy::default();
        let secs: Vec<u64> = p.nominal_schedule().iter().map(|d| d.as_secs()).collect();
        assert_eq!(secs, vec![30, 60, 120, 240, 480, 960, 1800]);
    }

    #[test]
    fn jitter_stays_within_bounds() {
        let p = RetryPolicy {
            base_delay: Duration::from_secs(30),
            max_delay: Duration::from_secs(1800),
            max_attempts: 8,
            jitter_ratio: 0.20,
        };
        for _ in 0..200 {
            let d = p.delay_after_failure(1).as_secs_f64();
            assert!((24.0..=36.0).contains(&d), "delay {d} out of bounds");
        }
        // 封顶后再加 jitter 不超过 30min * 1.2
        for _ in 0..50 {
            let d = p.delay_after_failure(7).as_secs_f64();
            assert!(d <= 1800.0 * 1.2 + 0.001);
        }
    }

    #[test]
    fn terminal_never_retries() {
        let p = RetryPolicy::default();
        assert!(!p.should_retry(1, ErrorClass::Terminal));
        assert!(p.should_retry(1, ErrorClass::Retryable));
        assert!(!p.should_retry(8, ErrorClass::Retryable));
    }
}
