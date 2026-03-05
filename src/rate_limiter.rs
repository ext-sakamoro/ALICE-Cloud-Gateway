//! Per-device レート制限 — Token Bucket アルゴリズム
//!
//! デバイスIDごとにリクエスト頻度を制限し、
//! `過負荷やDoSを防止する`。

use std::collections::HashMap;

/// レート制限設定。
#[derive(Debug, Clone, Copy)]
pub struct RateLimiterConfig {
    /// 1秒あたりの許可リクエスト数。
    pub requests_per_second: f64,
    /// バーストサイズ (最大トークン数)。
    pub burst_size: u32,
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        Self {
            requests_per_second: 100.0,
            burst_size: 200,
        }
    }
}

/// 単一のレートリミッター (Token Bucket)。
#[derive(Debug, Clone)]
pub struct RateLimiter {
    config: RateLimiterConfig,
    /// 現在のトークン数。
    tokens: f64,
    /// 最終更新時刻 (秒)。
    last_update: f64,
}

impl RateLimiter {
    /// 新しいリミッターを作成。
    #[must_use]
    pub fn new(config: RateLimiterConfig, now: f64) -> Self {
        Self {
            tokens: f64::from(config.burst_size),
            config,
            last_update: now,
        }
    }

    /// トークンを補充。
    fn refill(&mut self, now: f64) {
        let elapsed = (now - self.last_update).max(0.0);
        self.tokens = (self.tokens + elapsed * self.config.requests_per_second)
            .min(f64::from(self.config.burst_size));
        self.last_update = now;
    }

    /// リクエストを許可するか。許可されたらトークンを消費。
    pub fn try_acquire(&mut self, now: f64) -> bool {
        self.refill(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// 現在のトークン数。
    #[must_use]
    pub const fn available_tokens(&self) -> f64 {
        self.tokens
    }
}

/// デバイスIDごとのレートリミッター管理。
#[derive(Debug)]
pub struct DeviceRateLimiter {
    config: RateLimiterConfig,
    /// デバイスID → リミッター。
    limiters: HashMap<u64, RateLimiter>,
    /// 拒否されたリクエスト数 (全デバイス合計)。
    total_rejected: u64,
}

impl DeviceRateLimiter {
    /// 新しいデバイスリミッターを作成。
    #[must_use]
    pub fn new(config: RateLimiterConfig) -> Self {
        Self {
            config,
            limiters: HashMap::new(),
            total_rejected: 0,
        }
    }

    /// デフォルト設定で作成。
    #[must_use]
    pub fn with_defaults() -> Self {
        Self::new(RateLimiterConfig::default())
    }

    /// デバイスのリクエストを許可するか。
    pub fn try_acquire(&mut self, device_id: u64, now: f64) -> bool {
        let config = self.config;
        let limiter = self
            .limiters
            .entry(device_id)
            .or_insert_with(|| RateLimiter::new(config, now));

        let allowed = limiter.try_acquire(now);
        if !allowed {
            self.total_rejected += 1;
        }
        allowed
    }

    /// 登録済みデバイス数。
    #[must_use]
    pub fn device_count(&self) -> usize {
        self.limiters.len()
    }

    /// 拒否されたリクエスト総数。
    #[must_use]
    pub const fn total_rejected(&self) -> u64 {
        self.total_rejected
    }

    /// 特定デバイスのリミッターを削除。
    pub fn remove_device(&mut self, device_id: u64) -> bool {
        self.limiters.remove(&device_id).is_some()
    }

    /// 非アクティブデバイスを一括削除。
    ///
    /// `inactive_threshold_s` 秒以上更新がないデバイスを削除。
    pub fn purge_inactive(&mut self, now: f64, inactive_threshold_s: f64) -> usize {
        let before = self.limiters.len();
        self.limiters
            .retain(|_, v| (now - v.last_update) < inactive_threshold_s);
        before - self.limiters.len()
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
        let config = RateLimiterConfig::default();
        assert!((config.requests_per_second - 100.0).abs() < 1e-10);
        assert_eq!(config.burst_size, 200);
    }

    #[test]
    fn limiter_allows_within_burst() {
        let config = RateLimiterConfig {
            requests_per_second: 10.0,
            burst_size: 5,
        };
        let mut limiter = RateLimiter::new(config, 0.0);
        for _ in 0..5 {
            assert!(limiter.try_acquire(0.0));
        }
    }

    #[test]
    fn limiter_rejects_over_burst() {
        let config = RateLimiterConfig {
            requests_per_second: 10.0,
            burst_size: 3,
        };
        let mut limiter = RateLimiter::new(config, 0.0);
        for _ in 0..3 {
            assert!(limiter.try_acquire(0.0));
        }
        assert!(!limiter.try_acquire(0.0));
    }

    #[test]
    fn limiter_refills_over_time() {
        let config = RateLimiterConfig {
            requests_per_second: 10.0,
            burst_size: 5,
        };
        let mut limiter = RateLimiter::new(config, 0.0);
        // 全トークン消費
        for _ in 0..5 {
            limiter.try_acquire(0.0);
        }
        assert!(!limiter.try_acquire(0.0));

        // 0.5秒後 → 5トークン補充
        assert!(limiter.try_acquire(0.5));
    }

    #[test]
    fn limiter_tokens_capped_at_burst() {
        let config = RateLimiterConfig {
            requests_per_second: 100.0,
            burst_size: 10,
        };
        let mut limiter = RateLimiter::new(config, 0.0);
        limiter.refill(100.0); // 大量に時間が経過
        assert!(limiter.available_tokens() <= f64::from(config.burst_size) + 1e-10);
    }

    #[test]
    fn device_limiter_separate_buckets() {
        let config = RateLimiterConfig {
            requests_per_second: 10.0,
            burst_size: 2,
        };
        let mut dl = DeviceRateLimiter::new(config);

        // デバイス1: 2回OK、3回目NG
        assert!(dl.try_acquire(1, 0.0));
        assert!(dl.try_acquire(1, 0.0));
        assert!(!dl.try_acquire(1, 0.0));

        // デバイス2: 独立したバケットなのでOK
        assert!(dl.try_acquire(2, 0.0));
    }

    #[test]
    fn device_limiter_count() {
        let mut dl = DeviceRateLimiter::with_defaults();
        dl.try_acquire(1, 0.0);
        dl.try_acquire(2, 0.0);
        dl.try_acquire(3, 0.0);
        assert_eq!(dl.device_count(), 3);
    }

    #[test]
    fn device_limiter_total_rejected() {
        let config = RateLimiterConfig {
            requests_per_second: 10.0,
            burst_size: 1,
        };
        let mut dl = DeviceRateLimiter::new(config);
        dl.try_acquire(1, 0.0); // OK
        dl.try_acquire(1, 0.0); // NG
        dl.try_acquire(1, 0.0); // NG
        assert_eq!(dl.total_rejected(), 2);
    }

    #[test]
    fn device_limiter_remove() {
        let mut dl = DeviceRateLimiter::with_defaults();
        dl.try_acquire(1, 0.0);
        assert!(dl.remove_device(1));
        assert!(!dl.remove_device(1));
        assert_eq!(dl.device_count(), 0);
    }

    #[test]
    fn device_limiter_purge_inactive() {
        let mut dl = DeviceRateLimiter::with_defaults();
        dl.try_acquire(1, 0.0);
        dl.try_acquire(2, 0.0);
        dl.try_acquire(3, 100.0); // 最近のデバイス

        let purged = dl.purge_inactive(100.0, 60.0);
        assert_eq!(purged, 2); // デバイス1,2 が削除
        assert_eq!(dl.device_count(), 1);
    }

    #[test]
    fn device_limiter_purge_none() {
        let mut dl = DeviceRateLimiter::with_defaults();
        dl.try_acquire(1, 100.0);
        let purged = dl.purge_inactive(100.0, 60.0);
        assert_eq!(purged, 0);
    }

    #[test]
    fn limiter_available_tokens() {
        let config = RateLimiterConfig {
            requests_per_second: 10.0,
            burst_size: 5,
        };
        let limiter = RateLimiter::new(config, 0.0);
        assert!((limiter.available_tokens() - 5.0).abs() < 1e-10);
    }
}
