//! Link metrics tracking and Exponential Moving Average (EMA) smoothing.
//!
//! Provides a quantitative measure of link quality between peers to support
//! composite-cost routing decisions.

use serde::{Deserialize, Serialize};

/// Smoothing factor (alpha) for the Exponential Moving Average (EMA).
/// A value of 0.2 means 20% weight is given to the latest measurement.
const EMA_ALPHA: f64 = 0.2;

/// Performance metrics for a directed network link.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LinkMetrics {
    /// Latency/round-trip time in milliseconds.
    pub latency_ms: f64,
    /// Available throughput bandwidth in kilobits per second (Kbps).
    pub bandwidth_kbps: f64,
    /// Observed packet loss rate as a fraction between 0.0 and 1.0.
    pub loss_rate: f64,
    /// Monetary or resource routing cost weight.
    pub relay_cost: f64,
}

impl Default for LinkMetrics {
    fn default() -> Self {
        Self {
            latency_ms: 10.0,
            bandwidth_kbps: 10000.0, // 10 Mbps
            loss_rate: 0.0,
            relay_cost: 0.0,
        }
    }
}

impl LinkMetrics {
    /// Compute the composite cost score for this link.
    ///
    /// Formula:
    /// `cost = latency_ms * (1 + loss_rate) / (bandwidth_mbps).max(0.1) + relay_cost`
    ///
    /// Lower cost represents higher link quality.
    pub fn cost(&self) -> f64 {
        let bw_mbps = (self.bandwidth_kbps / 1000.0).max(0.1);
        self.latency_ms * (1.0 + self.loss_rate) / bw_mbps + self.relay_cost
    }
}

/// Dynamic tracker applying Exponential Moving Average (EMA) to smooth out link metrics.
#[derive(Debug, Clone)]
pub struct LinkMetricTracker {
    latency_ema: f64,
    bandwidth_ema: f64,
    loss_rate_ema: f64,
    relay_cost: f64,
}

impl Default for LinkMetricTracker {
    fn default() -> Self {
        let def = LinkMetrics::default();
        Self {
            latency_ema: def.latency_ms,
            bandwidth_ema: def.bandwidth_kbps,
            loss_rate_ema: def.loss_rate,
            relay_cost: def.relay_cost,
        }
    }
}

impl LinkMetricTracker {
    /// Create a new tracker seeded with initial metrics.
    pub fn new(initial: LinkMetrics) -> Self {
        Self {
            latency_ema: initial.latency_ms,
            bandwidth_ema: initial.bandwidth_kbps,
            loss_rate_ema: initial.loss_rate,
            relay_cost: initial.relay_cost,
        }
    }

    /// Update the tracked latency with a new measurement.
    pub fn record_latency(&mut self, measurement: f64) {
        self.latency_ema = EMA_ALPHA * measurement + (1.0 - EMA_ALPHA) * self.latency_ema;
    }

    /// Update the tracked bandwidth with a new measurement.
    pub fn record_bandwidth(&mut self, measurement: f64) {
        self.bandwidth_ema = EMA_ALPHA * measurement + (1.0 - EMA_ALPHA) * self.bandwidth_ema;
    }

    /// Update the tracked packet loss rate with a new measurement.
    pub fn record_loss_rate(&mut self, measurement: f64) {
        // Clamp to valid range [0.0, 1.0]
        let clamped = measurement.clamp(0.0, 1.0);
        self.loss_rate_ema = EMA_ALPHA * clamped + (1.0 - EMA_ALPHA) * self.loss_rate_ema;
    }

    /// Set a custom relay cost resource weight.
    pub fn set_relay_cost(&mut self, cost: f64) {
        self.relay_cost = cost;
    }

    /// Export the current smoothed metrics snapshot.
    pub fn metrics(&self) -> LinkMetrics {
        LinkMetrics {
            latency_ms: self.latency_ema,
            bandwidth_kbps: self.bandwidth_ema,
            loss_rate: self.loss_rate_ema,
            relay_cost: self.relay_cost,
        }
    }

    /// Compute the current composite cost score.
    pub fn cost(&self) -> f64 {
        self.metrics().cost()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composite_cost_calculation() {
        let metrics = LinkMetrics {
            latency_ms: 20.0,
            bandwidth_kbps: 2000.0, // 2 Mbps
            loss_rate: 0.5,         // 50% loss
            relay_cost: 5.0,
        };

        // cost = 20 * (1 + 0.5) / 2.0 + 5 = 20 * 1.5 / 2 + 5 = 15 + 5 = 20.0
        assert_eq!(metrics.cost(), 20.0);
    }

    #[test]
    fn test_zero_bandwidth_divisor_guard() {
        let metrics = LinkMetrics {
            latency_ms: 10.0,
            bandwidth_kbps: 0.0, // Guarded max(0.1) factor
            loss_rate: 0.0,
            relay_cost: 0.0,
        };
        // cost = 10 * 1 / 0.1 + 0 = 100.0
        assert_eq!(metrics.cost(), 100.0);
    }

    #[test]
    fn test_metric_tracker_ema_smoothing() {
        let mut tracker = LinkMetricTracker::new(LinkMetrics {
            latency_ms: 10.0,
            bandwidth_kbps: 1000.0,
            loss_rate: 0.0,
            relay_cost: 0.0,
        });

        // Record a latency spike
        tracker.record_latency(20.0);
        // ema = 0.2 * 20.0 + 0.8 * 10.0 = 4.0 + 8.0 = 12.0
        assert_eq!(tracker.metrics().latency_ms, 12.0);

        tracker.record_latency(20.0);
        // ema = 0.2 * 20.0 + 0.8 * 12.0 = 4.0 + 9.6 = 13.6
        assert!((tracker.metrics().latency_ms - 13.6).abs() < 1e-9);
    }
}
