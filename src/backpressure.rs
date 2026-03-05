//! フロー制御 — ウォーターマークベースのバックプレッシャー
//!
//! 下流サービス (DB/Sync/CDN) の飽和時にインジェスト速度を制御し、
//! メモリ圧迫やキュー溢れを防止する。

/// フロー制御アクション。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowAction {
    /// 通常処理を許可。
    Allow,
    /// スロットリング — 処理速度を抑制。
    Throttle,
    /// 拒否 — 新規リクエストを受け付けない。
    Reject,
}

/// バックプレッシャー設定。
#[derive(Debug, Clone, Copy)]
pub struct BackpressureConfig {
    /// 同時処理中の最大数。
    pub max_inflight: u64,
    /// スロットリング開始の閾値 (inflight / `max_inflight`)。
    pub high_watermark: f64,
    /// スロットリング解除の閾値。
    pub low_watermark: f64,
    /// キューの最大長。超過で Reject。
    pub max_queue_len: u64,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self {
            max_inflight: 10_000,
            high_watermark: 0.8,
            low_watermark: 0.5,
            max_queue_len: 50_000,
        }
    }
}

/// バックプレッシャーコントローラー。
#[derive(Debug)]
pub struct BackpressureController {
    config: BackpressureConfig,
    /// 現在の処理中リクエスト数。
    inflight: u64,
    /// 現在のキュー長。
    queue_len: u64,
    /// スロットリング中か。
    throttling: bool,
    /// 拒否されたリクエスト数 (統計)。
    rejected_count: u64,
    /// スロットリングされたリクエスト数 (統計)。
    throttled_count: u64,
}

impl BackpressureController {
    /// 新しいコントローラーを作成。
    #[must_use]
    pub const fn new(config: BackpressureConfig) -> Self {
        Self {
            config,
            inflight: 0,
            queue_len: 0,
            throttling: false,
            rejected_count: 0,
            throttled_count: 0,
        }
    }

    /// デフォルト設定で作成。
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self::new(BackpressureConfig {
            max_inflight: 10_000,
            high_watermark: 0.8,
            low_watermark: 0.5,
            max_queue_len: 50_000,
        })
    }

    /// 現在の状態を評価してアクションを返す。
    #[must_use]
    pub fn evaluate(&mut self) -> FlowAction {
        // キューが最大長を超過 → Reject
        if self.queue_len >= self.config.max_queue_len {
            self.rejected_count += 1;
            return FlowAction::Reject;
        }

        // inflight が最大を超過 → Reject
        if self.inflight >= self.config.max_inflight {
            self.rejected_count += 1;
            return FlowAction::Reject;
        }

        let ratio = if self.config.max_inflight > 0 {
            self.inflight as f64 / self.config.max_inflight as f64
        } else {
            0.0
        };

        // ヒステリシス: high を超えたらスロットリング開始、low を下回ったら解除
        if ratio >= self.config.high_watermark {
            self.throttling = true;
        } else if ratio <= self.config.low_watermark {
            self.throttling = false;
        }

        if self.throttling {
            self.throttled_count += 1;
            FlowAction::Throttle
        } else {
            FlowAction::Allow
        }
    }

    /// リクエスト開始を記録。
    pub const fn on_request_start(&mut self) {
        self.inflight += 1;
    }

    /// リクエスト完了を記録。
    pub const fn on_request_complete(&mut self) {
        self.inflight = self.inflight.saturating_sub(1);
    }

    /// キュー長を更新。
    pub const fn update_queue_len(&mut self, len: u64) {
        self.queue_len = len;
    }

    /// 現在の inflight 数。
    #[must_use]
    pub const fn inflight(&self) -> u64 {
        self.inflight
    }

    /// スロットリング中か。
    #[must_use]
    pub const fn is_throttling(&self) -> bool {
        self.throttling
    }

    /// 拒否されたリクエスト数。
    #[must_use]
    pub const fn rejected_count(&self) -> u64 {
        self.rejected_count
    }

    /// スロットリングされたリクエスト数。
    #[must_use]
    pub const fn throttled_count(&self) -> u64 {
        self.throttled_count
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
        let config = BackpressureConfig::default();
        assert_eq!(config.max_inflight, 10_000);
        assert!((config.high_watermark - 0.8).abs() < 1e-10);
    }

    #[test]
    fn allow_when_idle() {
        let mut ctrl = BackpressureController::with_defaults();
        assert_eq!(ctrl.evaluate(), FlowAction::Allow);
    }

    #[test]
    fn throttle_at_high_watermark() {
        let config = BackpressureConfig {
            max_inflight: 100,
            high_watermark: 0.8,
            low_watermark: 0.5,
            max_queue_len: 1000,
        };
        let mut ctrl = BackpressureController::new(config);
        for _ in 0..80 {
            ctrl.on_request_start();
        }
        assert_eq!(ctrl.evaluate(), FlowAction::Throttle);
    }

    #[test]
    fn reject_at_max_inflight() {
        let config = BackpressureConfig {
            max_inflight: 10,
            ..Default::default()
        };
        let mut ctrl = BackpressureController::new(config);
        for _ in 0..10 {
            ctrl.on_request_start();
        }
        assert_eq!(ctrl.evaluate(), FlowAction::Reject);
    }

    #[test]
    fn reject_at_max_queue() {
        let config = BackpressureConfig {
            max_queue_len: 5,
            ..Default::default()
        };
        let mut ctrl = BackpressureController::new(config);
        ctrl.update_queue_len(5);
        assert_eq!(ctrl.evaluate(), FlowAction::Reject);
    }

    #[test]
    fn hysteresis() {
        let config = BackpressureConfig {
            max_inflight: 100,
            high_watermark: 0.8,
            low_watermark: 0.5,
            max_queue_len: 10_000,
        };
        let mut ctrl = BackpressureController::new(config);

        // 80% → スロットリング開始
        for _ in 0..80 {
            ctrl.on_request_start();
        }
        assert_eq!(ctrl.evaluate(), FlowAction::Throttle);

        // 70% に下がってもまだスロットリング中 (low_watermark = 50%)
        for _ in 0..10 {
            ctrl.on_request_complete();
        }
        assert_eq!(ctrl.evaluate(), FlowAction::Throttle);

        // 49% に下がったらスロットリング解除
        for _ in 0..21 {
            ctrl.on_request_complete();
        }
        assert_eq!(ctrl.evaluate(), FlowAction::Allow);
    }

    #[test]
    fn request_lifecycle() {
        let mut ctrl = BackpressureController::with_defaults();
        ctrl.on_request_start();
        assert_eq!(ctrl.inflight(), 1);
        ctrl.on_request_complete();
        assert_eq!(ctrl.inflight(), 0);
    }

    #[test]
    fn complete_underflow_safe() {
        let mut ctrl = BackpressureController::with_defaults();
        ctrl.on_request_complete(); // 0 のまま
        assert_eq!(ctrl.inflight(), 0);
    }

    #[test]
    fn rejected_count_increments() {
        let config = BackpressureConfig {
            max_inflight: 1,
            ..Default::default()
        };
        let mut ctrl = BackpressureController::new(config);
        ctrl.on_request_start();
        let _ = ctrl.evaluate(); // Reject
        let _ = ctrl.evaluate(); // Reject
        assert_eq!(ctrl.rejected_count(), 2);
    }

    #[test]
    fn throttled_count_increments() {
        let config = BackpressureConfig {
            max_inflight: 10,
            high_watermark: 0.5,
            low_watermark: 0.3,
            max_queue_len: 10_000,
        };
        let mut ctrl = BackpressureController::new(config);
        for _ in 0..5 {
            ctrl.on_request_start();
        }
        let _ = ctrl.evaluate();
        let _ = ctrl.evaluate();
        assert_eq!(ctrl.throttled_count(), 2);
    }

    #[test]
    fn is_throttling_flag() {
        let ctrl = BackpressureController::with_defaults();
        assert!(!ctrl.is_throttling());
    }

    #[test]
    fn update_queue_len() {
        let mut ctrl = BackpressureController::with_defaults();
        ctrl.update_queue_len(100);
        assert_eq!(ctrl.evaluate(), FlowAction::Allow);
    }
}
