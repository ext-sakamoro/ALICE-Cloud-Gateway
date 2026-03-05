//! メトリクスエクスポート — Prometheus テキストフォーマット出力
//!
//! ゲートウェイの運用メトリクスを収集・登録し、
//! Prometheus 互換のテキストフォーマットで出力する。

use std::collections::BTreeMap;

/// メトリクス値の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricType {
    /// 累積カウンター (単調増加)。
    Counter,
    /// ゲージ (任意の値)。
    Gauge,
}

/// 単一のメトリクスエントリー。
#[derive(Debug, Clone)]
pub struct MetricEntry {
    /// メトリクス名。
    pub name: String,
    /// 説明文。
    pub help: String,
    /// メトリクス種類。
    pub metric_type: MetricType,
    /// ラベル付き値: `{label_set_key → value}`。
    /// ラベルなしの場合は空文字キー。
    values: BTreeMap<String, f64>,
}

impl MetricEntry {
    /// 新しいメトリクスを作成。
    #[must_use]
    pub fn new(name: &str, help: &str, metric_type: MetricType) -> Self {
        Self {
            name: name.to_string(),
            help: help.to_string(),
            metric_type,
            values: BTreeMap::new(),
        }
    }

    /// ラベルなしの値を設定。
    pub fn set(&mut self, value: f64) {
        self.values.insert(String::new(), value);
    }

    /// ラベルなしの値を加算 (Counter 用)。
    pub fn increment(&mut self, delta: f64) {
        let entry = self.values.entry(String::new()).or_insert(0.0);
        *entry += delta;
    }

    /// ラベル付きの値を設定。
    pub fn set_with_labels(&mut self, labels: &str, value: f64) {
        self.values.insert(labels.to_string(), value);
    }

    /// ラベル付きの値を加算。
    pub fn increment_with_labels(&mut self, labels: &str, delta: f64) {
        let entry = self.values.entry(labels.to_string()).or_insert(0.0);
        *entry += delta;
    }

    /// 値を取得。
    #[must_use]
    pub fn get(&self, labels: &str) -> Option<f64> {
        self.values.get(labels).copied()
    }
}

/// メトリクスレジストリ。
#[derive(Debug)]
pub struct MetricsRegistry {
    /// メトリクス名 → エントリー。
    metrics: BTreeMap<String, MetricEntry>,
}

impl MetricsRegistry {
    /// 空のレジストリを作成。
    #[must_use]
    pub const fn new() -> Self {
        Self {
            metrics: BTreeMap::new(),
        }
    }

    /// メトリクスを登録。
    pub fn register(&mut self, name: &str, help: &str, metric_type: MetricType) {
        self.metrics
            .entry(name.to_string())
            .or_insert_with(|| MetricEntry::new(name, help, metric_type));
    }

    /// カウンターを加算。
    pub fn counter_inc(&mut self, name: &str, delta: f64) {
        if let Some(entry) = self.metrics.get_mut(name) {
            entry.increment(delta);
        }
    }

    /// ゲージを設定。
    pub fn gauge_set(&mut self, name: &str, value: f64) {
        if let Some(entry) = self.metrics.get_mut(name) {
            entry.set(value);
        }
    }

    /// ラベル付きカウンターを加算。
    pub fn counter_inc_labels(&mut self, name: &str, labels: &str, delta: f64) {
        if let Some(entry) = self.metrics.get_mut(name) {
            entry.increment_with_labels(labels, delta);
        }
    }

    /// ラベル付きゲージを設定。
    pub fn gauge_set_labels(&mut self, name: &str, labels: &str, value: f64) {
        if let Some(entry) = self.metrics.get_mut(name) {
            entry.set_with_labels(labels, value);
        }
    }

    /// メトリクスの値を取得。
    #[must_use]
    pub fn get(&self, name: &str) -> Option<f64> {
        self.metrics.get(name).and_then(|e| e.get(""))
    }

    /// 登録済みメトリクス数。
    #[must_use]
    pub fn len(&self) -> usize {
        self.metrics.len()
    }

    /// 空か。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.metrics.is_empty()
    }

    /// Prometheus テキストフォーマットで出力。
    #[must_use]
    pub fn render_prometheus(&self) -> String {
        let mut out = String::new();

        for entry in self.metrics.values() {
            // # HELP line
            out.push_str("# HELP ");
            out.push_str(&entry.name);
            out.push(' ');
            out.push_str(&entry.help);
            out.push('\n');

            // # TYPE line
            out.push_str("# TYPE ");
            out.push_str(&entry.name);
            match entry.metric_type {
                MetricType::Counter => out.push_str(" counter\n"),
                MetricType::Gauge => out.push_str(" gauge\n"),
            }

            // Value lines
            for (labels, value) in &entry.values {
                out.push_str(&entry.name);
                if !labels.is_empty() {
                    out.push('{');
                    out.push_str(labels);
                    out.push('}');
                }
                out.push(' ');
                if value.fract() == 0.0 && value.abs() < 1e15 {
                    use std::fmt::Write;
                    let _ = write!(out, "{}", *value as i64);
                } else {
                    use std::fmt::Write;
                    let _ = write!(out, "{value}");
                }
                out.push('\n');
            }
        }

        out
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_get() {
        let mut reg = MetricsRegistry::new();
        reg.register("requests_total", "Total requests", MetricType::Counter);
        reg.counter_inc("requests_total", 5.0);
        assert!((reg.get("requests_total").unwrap() - 5.0).abs() < 1e-10);
    }

    #[test]
    fn gauge_set() {
        let mut reg = MetricsRegistry::new();
        reg.register("temperature", "Current temp", MetricType::Gauge);
        reg.gauge_set("temperature", 42.5);
        assert!((reg.get("temperature").unwrap() - 42.5).abs() < 1e-10);
    }

    #[test]
    fn counter_increment() {
        let mut reg = MetricsRegistry::new();
        reg.register("errors", "Error count", MetricType::Counter);
        reg.counter_inc("errors", 1.0);
        reg.counter_inc("errors", 2.0);
        assert!((reg.get("errors").unwrap() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn unregistered_metric_noop() {
        let mut reg = MetricsRegistry::new();
        reg.counter_inc("nonexistent", 1.0);
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn labeled_metrics() {
        let mut reg = MetricsRegistry::new();
        reg.register("http_requests", "HTTP requests", MetricType::Counter);
        reg.counter_inc_labels("http_requests", "method=\"GET\"", 10.0);
        reg.counter_inc_labels("http_requests", "method=\"POST\"", 3.0);

        let entry = reg.metrics.get("http_requests").unwrap();
        assert!((entry.get("method=\"GET\"").unwrap() - 10.0).abs() < 1e-10);
        assert!((entry.get("method=\"POST\"").unwrap() - 3.0).abs() < 1e-10);
    }

    #[test]
    fn len_and_empty() {
        let mut reg = MetricsRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);

        reg.register("m1", "help", MetricType::Counter);
        assert!(!reg.is_empty());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn prometheus_format_counter() {
        let mut reg = MetricsRegistry::new();
        reg.register("requests_total", "Total requests", MetricType::Counter);
        reg.counter_inc("requests_total", 42.0);

        let output = reg.render_prometheus();
        assert!(output.contains("# HELP requests_total Total requests"));
        assert!(output.contains("# TYPE requests_total counter"));
        assert!(output.contains("requests_total 42"));
    }

    #[test]
    fn prometheus_format_gauge() {
        let mut reg = MetricsRegistry::new();
        reg.register("queue_depth", "Queue depth", MetricType::Gauge);
        reg.gauge_set("queue_depth", 100.0);

        let output = reg.render_prometheus();
        assert!(output.contains("# TYPE queue_depth gauge"));
        assert!(output.contains("queue_depth 100"));
    }

    #[test]
    fn prometheus_format_with_labels() {
        let mut reg = MetricsRegistry::new();
        reg.register("http_requests", "HTTP requests", MetricType::Counter);
        reg.counter_inc_labels("http_requests", "method=\"GET\"", 5.0);

        let output = reg.render_prometheus();
        assert!(output.contains("http_requests{method=\"GET\"} 5"));
    }

    #[test]
    fn default_registry() {
        let reg = MetricsRegistry::default();
        assert!(reg.is_empty());
    }
}
