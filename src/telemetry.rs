//! Cloud Gateway Telemetry
//!
//! Collects streaming metrics using ALICE-Analytics probabilistic
//! data structures: DDSketch for latencies, HyperLogLog for unique
//! device counting, CountMinSketch for per-device packet frequency.
//!
//! Author: Moroya Sakamoto

use alice_analytics::{
    DDSketch2048, HyperLogLog16, CountMinSketch2048x7, FnvHasher,
};

/// Gateway telemetry collector
///
/// Uses probabilistic data structures for memory-efficient metrics:
/// - DDSketch: Latency percentiles (P50, P95, P99)
/// - HyperLogLog: Unique device count estimation
/// - CountMinSketch: Per-device packet frequency
#[derive(Clone)]
pub struct GatewayTelemetry {
    /// Total packets processed
    pub total_packets: u64,
    /// Total keyframes received
    pub keyframe_count: u64,
    /// Total delta packets received
    pub delta_count: u64,
    /// Total bytes received
    pub total_bytes: u64,
    /// Latency distribution (DDSketch, 1% relative error)
    latency_sketch: DDSketch2048,
    /// Unique device estimator (HyperLogLog, ~0.4% error)
    unique_devices: HyperLogLog16,
    /// Per-device packet frequency (Count-Min Sketch)
    device_frequency: CountMinSketch2048x7,
}

impl GatewayTelemetry {
    /// Create a new telemetry collector
    pub fn new() -> Self {
        Self {
            total_packets: 0,
            keyframe_count: 0,
            delta_count: 0,
            total_bytes: 0,
            latency_sketch: DDSketch2048::new(0.01),
            unique_devices: HyperLogLog16::new(),
            device_frequency: CountMinSketch2048x7::new(),
        }
    }

    /// Record a processed packet
    pub fn record_packet(
        &mut self,
        device_id: u64,
        packet_size: usize,
        latency_ms: f64,
        is_keyframe: bool,
    ) {
        self.total_packets += 1;
        self.total_bytes += packet_size as u64;

        self.keyframe_count += is_keyframe as u64;
        self.delta_count += (!is_keyframe) as u64;

        // Latency percentile tracking
        self.latency_sketch.insert(latency_ms);

        // Unique device estimation
        self.unique_devices.insert_hash(FnvHasher::hash_u64(device_id));

        // Per-device frequency
        self.device_frequency.insert_hash(FnvHasher::hash_u64(device_id));
    }

    /// Estimated P50 latency (ms)
    pub fn p50_latency_ms(&self) -> f64 {
        self.latency_sketch.quantile(0.50)
    }

    /// Estimated P95 latency (ms)
    pub fn p95_latency_ms(&self) -> f64 {
        self.latency_sketch.quantile(0.95)
    }

    /// Estimated P99 latency (ms)
    pub fn p99_latency_ms(&self) -> f64 {
        self.latency_sketch.quantile(0.99)
    }

    /// Estimated unique device count
    pub fn estimated_unique_devices(&self) -> f64 {
        self.unique_devices.cardinality()
    }

    /// Estimated packet count for a specific device
    pub fn device_packet_estimate(&self, device_id: u64) -> u32 {
        self.device_frequency.estimate_hash(FnvHasher::hash_u64(device_id))
    }

    /// Average packet size (bytes)
    pub fn avg_packet_size(&self) -> f64 {
        if self.total_packets == 0 {
            return 0.0;
        }
        self.total_bytes as f64 / self.total_packets as f64
    }

    /// Keyframe ratio (0.0 - 1.0)
    pub fn keyframe_ratio(&self) -> f64 {
        if self.total_packets == 0 {
            return 0.0;
        }
        self.keyframe_count as f64 / self.total_packets as f64
    }

    /// Generate summary report
    pub fn summary(&self) -> TelemetrySummary {
        TelemetrySummary {
            total_packets: self.total_packets,
            keyframe_count: self.keyframe_count,
            delta_count: self.delta_count,
            total_bytes: self.total_bytes,
            avg_packet_size: self.avg_packet_size(),
            keyframe_ratio: self.keyframe_ratio(),
            p50_latency_ms: self.p50_latency_ms(),
            p95_latency_ms: self.p95_latency_ms(),
            p99_latency_ms: self.p99_latency_ms(),
            estimated_unique_devices: self.estimated_unique_devices(),
        }
    }
}

/// Telemetry summary snapshot
#[derive(Debug, Clone)]
pub struct TelemetrySummary {
    pub total_packets: u64,
    pub keyframe_count: u64,
    pub delta_count: u64,
    pub total_bytes: u64,
    pub avg_packet_size: f64,
    pub keyframe_ratio: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub estimated_unique_devices: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telemetry_basic() {
        let mut tel = GatewayTelemetry::new();
        assert_eq!(tel.total_packets, 0);

        tel.record_packet(1, 500, 2.5, true);
        assert_eq!(tel.total_packets, 1);
        assert_eq!(tel.keyframe_count, 1);
        assert_eq!(tel.total_bytes, 500);
    }

    #[test]
    fn test_telemetry_mixed_packets() {
        let mut tel = GatewayTelemetry::new();

        // 10 keyframes from device 1
        for i in 0..10 {
            tel.record_packet(1, 1000, 2.0 + i as f64 * 0.1, true);
        }

        // 90 deltas from device 2
        for i in 0..90 {
            tel.record_packet(2, 200, 1.0 + i as f64 * 0.01, false);
        }

        assert_eq!(tel.total_packets, 100);
        assert_eq!(tel.keyframe_count, 10);
        assert_eq!(tel.delta_count, 90);
        assert!((tel.keyframe_ratio() - 0.1).abs() < 0.01);
    }

    #[test]
    fn test_telemetry_latency_percentiles() {
        let mut tel = GatewayTelemetry::new();

        // Insert 100 latency samples: 1.0, 2.0, ..., 100.0
        for i in 1..=100 {
            tel.record_packet(1, 100, i as f64, false);
        }

        let p50 = tel.p50_latency_ms();
        let p99 = tel.p99_latency_ms();

        // P50 should be ~50, P99 should be ~99
        assert!(p50 > 40.0 && p50 < 60.0, "P50 = {}", p50);
        assert!(p99 > 90.0 && p99 < 105.0, "P99 = {}", p99);
    }

    #[test]
    fn test_telemetry_unique_devices() {
        let mut tel = GatewayTelemetry::new();

        // 5 unique devices, 20 packets each
        for device_id in 1..=5 {
            for _ in 0..20 {
                tel.record_packet(device_id, 100, 1.0, false);
            }
        }

        let estimate = tel.estimated_unique_devices();
        // HLL should estimate ~5 ± error
        assert!(estimate > 3.0 && estimate < 8.0, "Estimate = {}", estimate);
    }

    #[test]
    fn test_telemetry_device_frequency() {
        let mut tel = GatewayTelemetry::new();

        // Device 1: 100 packets, Device 2: 10 packets
        for _ in 0..100 {
            tel.record_packet(1, 100, 1.0, false);
        }
        for _ in 0..10 {
            tel.record_packet(2, 100, 1.0, false);
        }

        let freq1 = tel.device_packet_estimate(1);
        let freq2 = tel.device_packet_estimate(2);

        // CMS guarantees freq >= true count
        assert!(freq1 >= 100);
        assert!(freq2 >= 10);
        // Should be close to true values
        assert!(freq1 < 120, "Freq1 = {}", freq1);
        assert!(freq2 < 20, "Freq2 = {}", freq2);
    }

    #[test]
    fn test_telemetry_summary() {
        let mut tel = GatewayTelemetry::new();
        tel.record_packet(1, 500, 5.0, true);
        tel.record_packet(2, 200, 2.0, false);

        let summary = tel.summary();
        assert_eq!(summary.total_packets, 2);
        assert_eq!(summary.total_bytes, 700);
        assert!((summary.avg_packet_size - 350.0).abs() < 0.01);
    }
}
