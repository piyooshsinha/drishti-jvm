//! # JVM Tuning Recommendations
//!
//! Rules-based recommendation engine that observes JVM metrics and produces
//! actionable tuning suggestions (heap sizing, GC selection, HikariCP, etc.).
//!
//! Each rule has a stable id, trigger condition, suggestion, and confidence score.
//! Rules are implemented in Phase 6. This module defines the contract.

use crate::anomaly::Severity;
use crate::model::JvmSnapshot;
use serde::{Deserialize, Serialize};

/// Category of tuning recommendation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    HeapSizing,
    GcSelection,
    G1Tuning,
    ZgcTuning,
    ShenandoahTuning,
    Metaspace,
    ThreadPool,
    HikariCp,
    CodeLevel,
}

/// A single tuning recommendation with copy-pasteable JVM flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Stable machine-readable id (e.g., "increase_xmx", "switch_to_zgc").
    pub id: String,
    pub category: Category,
    pub severity: Severity,
    pub title: String,
    /// What the engine observed (e.g., "Old Gen post-GC: 78% of Xmx over last 20 GCs").
    pub current_state: String,
    /// What to change (e.g., "Increase -Xmx from 512m to 768m").
    pub suggestion: String,
    /// Why (e.g., "Oracle recommends post-GC old-gen < 65% of Xmx").
    pub rationale: String,
    /// Confidence (0.0–1.0) derived from R², sample size, trigger duration.
    pub confidence: f64,
    /// Copy-pasteable JVM flags (e.g., ["-Xmx768m", "-Xms768m"]).
    pub jvm_flags: Vec<String>,
}

/// Trait for individual tuning rules.
pub trait TuningRule: Send + Sync {
    /// Evaluate whether this rule fires given current + historical data.
    fn evaluate(&self, current: &JvmSnapshot, history: &[JvmSnapshot]) -> Option<Recommendation>;

    /// Stable rule id for config-based enable/disable.
    fn id(&self) -> &str;

    /// Human-readable name for logging.
    fn name(&self) -> &str;
}

/// Registry of all tuning rules.
#[derive(Default)]
pub struct RecommendationEngine {
    rules: Vec<Box<dyn TuningRule>>,
    /// Minimum confidence threshold — rules below this are suppressed.
    pub min_confidence: f64,
}

impl RecommendationEngine {
    pub fn new(min_confidence: f64) -> Self {
        Self {
            rules: Vec::new(),
            min_confidence,
        }
    }

    pub fn register(&mut self, rule: Box<dyn TuningRule>) {
        self.rules.push(rule);
    }

    /// Run all rules and return recommendations above the confidence threshold.
    pub fn evaluate(
        &self,
        current: &JvmSnapshot,
        history: &[JvmSnapshot],
    ) -> Vec<Recommendation> {
        self.rules
            .iter()
            .filter_map(|rule| {
                let rec = rule.evaluate(current, history)?;
                if rec.confidence >= self.min_confidence {
                    Some(rec)
                } else {
                    None
                }
            })
            .collect()
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_engine_returns_nothing() {
        let engine = RecommendationEngine::new(0.5);
        let snap = JvmSnapshot::default();
        let recs = engine.evaluate(&snap, &[]);
        assert!(recs.is_empty());
    }
}
