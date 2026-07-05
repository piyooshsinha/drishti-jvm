//! # Anomaly Detection
//!
//! Trait-based anomaly detection system. Each detector evaluates a window
//! of JvmSnapshots and produces alerts with severity, evidence, and
//! suppression logic.
//!
//! Detectors are implemented in Phase 6. This module defines the contract.

use crate::model::JvmSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Alert severity levels, ordered by urgency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    Info,
    Warn,
    High,
    Critical,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Severity::Info => write!(f, "INFO"),
            Severity::Warn => write!(f, "WARN"),
            Severity::High => write!(f, "HIGH"),
            Severity::Critical => write!(f, "CRIT"),
        }
    }
}

/// Which TUI tab contains the evidence for this alert.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvidenceTab {
    Overview,
    Memory,
    Threads,
    Http,
    Db,
    Logs,
    Recommendations,
}

/// A detected anomaly with actionable context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// Stable machine-readable id (e.g., "mem_leak_detected", "gc_throughput_low").
    pub id: String,
    pub severity: Severity,
    pub title: String,
    pub detail: String,
    /// Which tab to jump to for evidence.
    pub evidence_tab: EvidenceTab,
    pub first_seen: DateTime<Utc>,
    /// Don't fire again until this time.
    pub suppressed_until: Option<DateTime<Utc>>,
    /// Confidence score (0.0–1.0) based on R², sample size, trigger duration.
    pub confidence: f64,
}

/// Trait for anomaly detectors.
///
/// Each implementation encodes a single detection rule (or small family).
/// The engine runs all registered detectors on each evaluation tick.
pub trait AnomalyDetector: Send + Sync {
    /// Evaluate the current snapshot plus historical context.
    /// Returns zero or more alerts.
    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Vec<Alert>;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

/// Registry of all active anomaly detectors.
#[derive(Default)]
pub struct AnomalyEngine {
    detectors: Vec<Box<dyn AnomalyDetector>>,
}

impl AnomalyEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, detector: Box<dyn AnomalyDetector>) {
        self.detectors.push(detector);
    }

    /// Run all detectors and return combined alerts.
    pub fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Vec<Alert> {
        let mut alerts = Vec::new();
        for detector in &self.detectors {
            let mut results = detector.evaluate(current, history);
            alerts.append(&mut results);
        }
        // Sort by severity (Critical first)
        alerts.sort_by_key(|a| std::cmp::Reverse(a.severity));
        alerts
    }

    pub fn detector_count(&self) -> usize {
        self.detectors.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severity_ordering() {
        assert!(Severity::Critical > Severity::High);
        assert!(Severity::High > Severity::Warn);
        assert!(Severity::Warn > Severity::Info);
    }

    #[test]
    fn empty_engine_returns_no_alerts() {
        let engine = AnomalyEngine::new();
        let snap = JvmSnapshot::default();
        let alerts = engine.evaluate(&snap, &[]);
        assert!(alerts.is_empty());
    }
}
