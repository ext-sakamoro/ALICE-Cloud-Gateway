//! サーキットブレーカー — 下流サービス障害時の fail-fast
//!
//! 連続失敗を検出して回路を開き、一定時間後に半開で試行、
//! 成功すれば回路を閉じるパターンを実装する。

/// 回路の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    /// 正常稼働中。
    Closed,
    /// 障害検出 — リクエストを即座に拒否。
    Open,
    /// 試行中 — 少数のリクエストを通して回復を確認。
    HalfOpen,
}

/// サーキットブレーカー設定。
#[derive(Debug, Clone, Copy)]
pub struct CircuitBreakerConfig {
    /// 回路を開くまでの連続失敗回数。
    pub failure_threshold: u32,
    /// Open → `HalfOpen` への遷移待ち時間 (秒)。
    pub recovery_timeout_s: f64,
    /// `HalfOpen` 状態で許可する最大リクエスト数。
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            recovery_timeout_s: 30.0,
            half_open_max_requests: 3,
        }
    }
}

/// サーキットブレーカー。
#[derive(Debug)]
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitState,
    /// 連続失敗回数。
    consecutive_failures: u32,
    /// Open になった時刻 (秒)。
    opened_at: f64,
    /// `HalfOpen` でのリクエスト数。
    half_open_requests: u32,
    /// `HalfOpen` での成功数。
    half_open_successes: u32,
    /// 状態遷移回数 (統計)。
    transition_count: u64,
}

impl CircuitBreaker {
    /// 新しいサーキットブレーカーを作成。
    #[must_use]
    pub const fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            state: CircuitState::Closed,
            consecutive_failures: 0,
            opened_at: 0.0,
            half_open_requests: 0,
            half_open_successes: 0,
            transition_count: 0,
        }
    }

    /// デフォルト設定で作成。
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self::new(CircuitBreakerConfig {
            failure_threshold: 5,
            recovery_timeout_s: 30.0,
            half_open_max_requests: 3,
        })
    }

    /// 現在の状態。
    #[must_use]
    pub const fn state(&self) -> CircuitState {
        self.state
    }

    /// リクエストを許可するか判定。
    ///
    /// `now` は現在時刻 (秒)。
    pub fn allow_request(&mut self, now: f64) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                // タイムアウト経過 → HalfOpen へ遷移
                if (now - self.opened_at) >= self.config.recovery_timeout_s {
                    self.transition_to(CircuitState::HalfOpen);
                    self.half_open_requests = 1;
                    self.half_open_successes = 0;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                if self.half_open_requests < self.config.half_open_max_requests {
                    self.half_open_requests += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    /// 成功を記録。
    pub fn record_success(&mut self) {
        match self.state {
            CircuitState::Closed => {
                self.consecutive_failures = 0;
            }
            CircuitState::HalfOpen => {
                self.half_open_successes += 1;
                // 全試行が成功 → Closed へ
                if self.half_open_successes >= self.config.half_open_max_requests {
                    self.transition_to(CircuitState::Closed);
                    self.consecutive_failures = 0;
                }
            }
            CircuitState::Open => {}
        }
    }

    /// 失敗を記録。
    pub fn record_failure(&mut self, now: f64) {
        match self.state {
            CircuitState::Closed => {
                self.consecutive_failures += 1;
                if self.consecutive_failures >= self.config.failure_threshold {
                    self.transition_to(CircuitState::Open);
                    self.opened_at = now;
                }
            }
            CircuitState::HalfOpen => {
                // HalfOpen で失敗 → 再び Open
                self.transition_to(CircuitState::Open);
                self.opened_at = now;
            }
            CircuitState::Open => {}
        }
    }

    /// 連続失敗回数。
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// 状態遷移回数。
    #[must_use]
    pub const fn transition_count(&self) -> u64 {
        self.transition_count
    }

    /// 手動リセット (Closed に戻す)。
    pub fn reset(&mut self) {
        self.transition_to(CircuitState::Closed);
        self.consecutive_failures = 0;
        self.half_open_requests = 0;
        self.half_open_successes = 0;
    }

    /// 状態遷移。
    fn transition_to(&mut self, new_state: CircuitState) {
        if self.state != new_state {
            self.state = new_state;
            self.transition_count += 1;
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert!((config.recovery_timeout_s - 30.0).abs() < 1e-10);
        assert_eq!(config.half_open_max_requests, 3);
    }

    #[test]
    fn starts_closed() {
        let cb = CircuitBreaker::with_defaults();
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn allows_in_closed() {
        let mut cb = CircuitBreaker::with_defaults();
        assert!(cb.allow_request(0.0));
    }

    #[test]
    fn opens_after_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new(config);

        for i in 0..3 {
            cb.record_failure(f64::from(i));
        }
        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn rejects_in_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_s: 10.0,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new(config);
        cb.record_failure(0.0);
        assert_eq!(cb.state(), CircuitState::Open);
        assert!(!cb.allow_request(5.0)); // タイムアウト前
    }

    #[test]
    fn transitions_to_half_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_s: 10.0,
            half_open_max_requests: 2,
        };
        let mut cb = CircuitBreaker::new(config);
        cb.record_failure(0.0);
        assert_eq!(cb.state(), CircuitState::Open);

        // タイムアウト後
        assert!(cb.allow_request(11.0));
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn half_open_to_closed_on_success() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_s: 5.0,
            half_open_max_requests: 2,
        };
        let mut cb = CircuitBreaker::new(config);
        cb.record_failure(0.0);

        // HalfOpen へ
        cb.allow_request(6.0);
        cb.record_success();
        cb.allow_request(6.0);
        cb.record_success();

        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn half_open_to_open_on_failure() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_s: 5.0,
            half_open_max_requests: 3,
        };
        let mut cb = CircuitBreaker::new(config);
        cb.record_failure(0.0);

        // HalfOpen へ
        cb.allow_request(6.0);
        cb.record_failure(6.0);

        assert_eq!(cb.state(), CircuitState::Open);
    }

    #[test]
    fn half_open_limits_requests() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_s: 5.0,
            half_open_max_requests: 2,
        };
        let mut cb = CircuitBreaker::new(config);
        cb.record_failure(0.0);

        cb.allow_request(6.0); // 1st → true (遷移時)
        assert!(cb.allow_request(6.0)); // 2nd → true
        assert!(!cb.allow_request(6.0)); // 3rd → false (上限超過)
    }

    #[test]
    fn success_resets_failures_in_closed() {
        let mut cb = CircuitBreaker::with_defaults();
        cb.record_failure(0.0);
        cb.record_failure(1.0);
        assert_eq!(cb.consecutive_failures(), 2);
        cb.record_success();
        assert_eq!(cb.consecutive_failures(), 0);
    }

    #[test]
    fn manual_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            ..Default::default()
        };
        let mut cb = CircuitBreaker::new(config);
        cb.record_failure(0.0);
        assert_eq!(cb.state(), CircuitState::Open);

        cb.reset();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.consecutive_failures(), 0);
    }

    #[test]
    fn transition_count_tracks() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            recovery_timeout_s: 5.0,
            half_open_max_requests: 1,
        };
        let mut cb = CircuitBreaker::new(config);
        assert_eq!(cb.transition_count(), 0);

        cb.record_failure(0.0); // Closed → Open
        assert_eq!(cb.transition_count(), 1);

        cb.allow_request(6.0); // Open → HalfOpen
        assert_eq!(cb.transition_count(), 2);
    }
}
